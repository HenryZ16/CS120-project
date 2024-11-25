use std::{mem, u32, u64, vec};

use crate::{
    acoustic_mac::mac_frame::{self, MACFrame},
    acoustic_modem::{
        demodulation::{DemodulationState, SwitchSignal},
        generator::PhyLayerGenerator,
    },
    asio_stream::InputAudioStream,
    utils::{get_audio_device_and_config, Byte},
};
use anyhow::Error;
use cpal::Device;
use cpal::SupportedStreamConfig;
use futures::StreamExt;
use std::collections::VecDeque;
use std::result::Result::Ok;
use std::time::{Duration, Instant};
use tokio::{sync::watch, time::error::Elapsed};
use tokio::{
    sync::{
        mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
        oneshot::{self, channel, Receiver, Sender},
    },
    time::timeout,
};

use super::{
    mac_frame::{MACType, MacAddress},
    send::MacSender,
};
use rand::{rngs::StdRng, Rng, SeedableRng};

const MAX_SEND: u64 = 30;
const ACK_WAIT_TIME: u64 = 30;
const BACKOFF_SLOT_TIME: u64 = 70;
const BACKOFF_MAX_FACTOR: u64 = 6;
const RECV_TIME: u64 = 27;

const DETECT_SIGNAL: Byte = 1;
const ENERGE_LIMIT: f32 = 0.005;

#[derive(PartialEq)]
enum TimerType {
    ACK,
    BACKOFF,
    None,
}
struct RecordTimer {
    start_instant: Instant,
    duration: Duration, // ms
    timer_type: TimerType,
    rng: StdRng,
}

impl RecordTimer {
    fn new() -> Self {
        let start_instant = Instant::now();
        let rng = StdRng::from_entropy();

        Self {
            start_instant,
            duration: Duration::new(u64::MAX, 0),
            timer_type: TimerType::None,
            rng,
        }
    }

    // arg: duration in ms
    fn start(&mut self, timer_type: TimerType, factor: u64, _continue_sends: u64) {
        self.start_instant = Instant::now();

        self.duration = match timer_type {
            TimerType::BACKOFF => {
                let factor = if (1 << factor) > BACKOFF_MAX_FACTOR {
                    BACKOFF_MAX_FACTOR
                } else {
                    1 << factor
                };
                let slot_times: u64 = self.rng.gen_range(0..=factor);
                // if continue_sends > 4 {
                //     slot_times *= 2;
                // }
                Duration::from_millis(BACKOFF_SLOT_TIME * slot_times)
            }
            TimerType::ACK => Duration::from_millis(ACK_WAIT_TIME),
            _ => Duration::from_micros(1),
        };
        self.timer_type = timer_type;
    }

    fn is_timeout(&self) -> bool {
        self.start_instant.elapsed() > self.duration
    }
}

pub struct MacController {
    phy_config: PhyLayerGenerator,
    mac_address: MacAddress,
}

impl MacController {
    pub fn new(config_file: &str, mac_address: MacAddress) -> Self {
        let phy_config = PhyLayerGenerator::new_from_yaml(config_file);

        Self {
            phy_config,
            mac_address,
        }
    }

    pub async fn task(
        &mut self,
        receive_output: &mut Vec<Byte>,
        receive_byte_num: usize,
        send_data: Vec<Byte>,
        dest: MacAddress,
    ) -> Result<(), anyhow::Error> {
        let start = Instant::now();
        let (decoded_data_tx, mut decoded_data_rx) = unbounded_channel();
        let (demodulate_status_tx, demodulate_status_rx) = unbounded_channel();

        let init_state = DemodulationState::DetectPreamble;
        let (mut detector, request_rx, result_tx) = MacDetector::new().await;
        let (device, config) = get_audio_device_and_config(self.phy_config.get_sample_rate());
        let mut demodulator = self
            .phy_config
            .gen_demodulation(device.clone(), config.clone());
        // start decode listening
        let _listen_task =
            demodulator.listening_daemon(decoded_data_tx, demodulate_status_rx, init_state);
        let _detector_daemon =
            MacDetector::daemon(request_rx, result_tx, device.clone(), config.clone());
        let mut sender =
            MacSender::new_from_genrator(&self.phy_config, self.mac_address, device, config);

        // setup timer
        let mut timer = RecordTimer::new();

        let mut send_padding: bool = false;
        let mut recv_padding: bool = false;
        let mac_address = self.mac_address;
        println!("set up time: {:?}", start.elapsed());
        let main_task = tokio::spawn(async move {
            let mut received: Vec<Byte> = vec![];
            let mut retry_times: u64 = 0;
            let mut resend_times: u64 = 0;
            let mut continue_sends: u64 = 0;
            let ack_frame = sender.generate_ack_frame(dest);
            let send_frame = sender.generate_digital_data_frames(send_data, dest);
            let tmp = sender.generate_ack_frame(u8::MAX);
            sender.send_frame(&tmp).await;
            let mut cur_send_frame: usize = 0;
            let mut cur_recv_frame: usize = 0;

            if send_frame.len() > 0 {
                timer.start(TimerType::BACKOFF, 0, continue_sends);
                send_padding = true;
                println!("frames to send: {}", send_frame.len());
            }
            if receive_byte_num > 0 {
                recv_padding = true;
            }

            let mut t_rtt_start = Instant::now();
            while send_padding || recv_padding {
                // if let Ok(data) = decoded_data_rx.try_recv() {
                if let Ok(Some(data)) =
                    timeout(Duration::from_millis(RECV_TIME), decoded_data_rx.recv()).await
                {
                    // check data type
                    if mac_frame::MACFrame::get_dst(&data) == mac_address {
                        // println!("[Controller]: received data: {:?}", data);
                        if mac_frame::MACFrame::get_type(&data) == MACType::Ack {
                            cur_send_frame += 1;
                            if cur_send_frame == send_frame.len() {
                                println!("cur send frame: {} and stopped", cur_send_frame);
                                send_padding = false;
                            } else if send_frame.len() > cur_send_frame {
                                retry_times = 0;
                                resend_times = 0;
                                continue_sends += 1;
                                println!(
                                    "send frame {} success, RTT: {:?}",
                                    cur_send_frame - 1,
                                    t_rtt_start.elapsed()
                                );
                                timer.start(TimerType::BACKOFF, 0, continue_sends);
                            }
                        } else {
                            MacController::send_frame(
                                &demodulate_status_tx,
                                &mut detector,
                                &mut sender,
                                &ack_frame,
                                false,
                            )
                            .await;
                            if (cur_recv_frame & 0x3F) as u8 == MACFrame::get_frame_id(&data) {
                                if data.len() < 5 {
                                    println!("[MacController]: received NONE frame");
                                    continue;
                                } else {
                                    println!(
                                        "[MacController]: received frame id: {}",
                                        cur_recv_frame
                                    );
                                    cur_recv_frame += 1;
                                    continue_sends = 0;
                                    received.extend(MACFrame::get_payload(&data));
                                    if received.len() >= receive_byte_num {
                                        println!("received length: {} and stopped", received.len());
                                        recv_padding = false;
                                    }
                                }
                            } else {
                                println!(
                                    "[MacController]: expected frame id: {}, received id: {}",
                                    if cur_recv_frame == 0 {
                                        u8::MAX as usize
                                    } else {
                                        cur_recv_frame
                                    },
                                    MACFrame::get_frame_id(&data)
                                );
                            }
                        }
                    } else {
                        println!(
                            "[MacController]: received other macaddress: {}",
                            mac_frame::MACFrame::get_dst(&data)
                        );
                    }
                }

                if send_padding && timer.is_timeout() {
                    match timer.timer_type {
                        TimerType::ACK => {
                            println!(
                                "[MacController]: ACK timeout times: {} on frame {}",
                                retry_times, cur_send_frame
                            );
                            retry_times += 1;
                            if retry_times >= MAX_SEND {
                                return Err(Error::msg("link error"));

                                // for test
                                // retry_times = 0;
                                // timer.start(TimerType::BACKOFF, retry_times);
                                // cur_send_frame += 1;
                                // if cur_send_frame == send_frame.len() {
                                //     return Err(Error::msg("link error"));
                                // }
                                // continue;
                            }

                            timer.start(TimerType::BACKOFF, 0, continue_sends);
                        }
                        TimerType::BACKOFF => {
                            t_rtt_start = Instant::now();
                            if MacController::send_frame(
                                &demodulate_status_tx,
                                &mut detector,
                                &mut sender,
                                &send_frame[cur_send_frame],
                                // &ack_frame,
                                true,
                            )
                            .await
                            {
                                timer.start(TimerType::ACK, 0, continue_sends);
                                // println!("send a frame: {:?}", t_rtt_start.elapsed());
                            } else {
                                println!(
                                    "[MacController]: busy channel, send frame {} failed, set backoff",
                                    cur_send_frame
                                );
                                resend_times += 1;
                                timer.start(TimerType::BACKOFF, resend_times, continue_sends);
                            }
                        }
                        _ => {}
                    }
                }
            }
            return Ok(received);
        });

        let handle = tokio::select! {
            _ = _detector_daemon => {vec![]}
            _ = _listen_task => {vec![]}
            Ok(data) = main_task => {
                if let Ok(data) = data{
                    data
                }
                else
                {
                    println!("{}", data.unwrap_err());
                    vec![]
                }
            }
        };
        receive_output.extend(handle.iter());
        println!("[MacController] task end");
        Ok(())
    }

    pub async fn mac_daemon(
        &mut self,
        mut send_task_rx: UnboundedReceiver<MacSendTask>,
        mut recv_task_tx: UnboundedSender<Vec<Byte>>,
    ) {
        let (decoded_data_tx, mut decoded_data_rx) = unbounded_channel();
        let (demodulate_status_tx, demodulate_status_rx) = unbounded_channel();

        let init_state = DemodulationState::DetectPreamble;
        let (mut detector, request_rx, result_tx) = MacDetector::new().await;
        let (device, config) = get_audio_device_and_config(self.phy_config.get_sample_rate());
        let mut demodulator = self
            .phy_config
            .gen_demodulation(device.clone(), config.clone());
        // start decode listening
        let _listen_task =
            demodulator.listening_daemon(decoded_data_tx, demodulate_status_rx, init_state);
        let _detector_daemon =
            MacDetector::daemon(request_rx, result_tx, device.clone(), config.clone());
        let mut sender =
            MacSender::new_from_genrator(&self.phy_config, self.mac_address, device, config);
        let mac_address = self.mac_address;
        let main_task = tokio::spawn(async move {
            let mut receive_tasks: VecDeque<MacReceiveTask> = VecDeque::with_capacity(10);
            let mut send_tasks: VecDeque<MacSendTask> = VecDeque::with_capacity(10);
            let tmp = sender.generate_ack_frame(u8::MAX);
            sender.send_frame(&tmp).await;
            let mut cur_send_task = None;
            let mut t_rtt_start = Instant::now();
            loop {
                while let Ok(mut recv_task) = recv_task_tx.try_recv() {
                    receive_tasks.push_back(recv_task);
                }
                while let Ok(mut send_task) = send_task_rx.try_recv() {
                    let ack_frame = sender.generate_ack_frame(send_task.dst);
                    send_task.fresh_send_frame(
                        sender.generate_digital_data_frames(send_task.to_sends[0], send_task.dst),
                    );
                    send_tasks.push_back(send_task);
                }
                if cur_send_task.is_none() {
                    cur_send_task = send_tasks.pop_front();
                }

                if let Ok(Some(data)) =
                    timeout(Duration::from_millis(RECV_TIME), decoded_data_rx.recv()).await
                {
                    // check data type
                    if mac_frame::MACFrame::get_dst(&data) == mac_address {
                        // println!("[Controller]: received data: {:?}", data);
                        if mac_frame::MACFrame::get_type(&data) == MACType::Ack {
                            if let Some(mut task) = cur_send_task {
                                task.cur_frame += 1;
                                if task.cur_frame == task.to_sends.len() {
                                    println!("[MacDaemon]: complete 1 send task to {}", task.dst);
                                    task.endsignal_tx.send(true);
                                } else if task.to_send_macframe.len() > task.cur_frame {
                                    task.retry_times = 0;
                                    task.resend_times = 0;
                                    println!(
                                        "send frame {} success, RTT: {:?}",
                                        task.cur_frame - 1,
                                        t_rtt_start.elapsed()
                                    );
                                    task.timer.start(TimerType::BACKOFF, 0, 0);
                                }
                            } else {
                                println!("[MacDaemon]: !!! No send task handle ACK");
                            }
                        } else {
                            let ack_frame = sender.generate_ack_frame(MACFrame::get_src(&data));
                            for task in receive_tasks {}

                            MacController::send_frame(
                                &demodulate_status_tx,
                                &mut detector,
                                &mut sender,
                                &ack_frame,
                                false,
                            )
                            .await;

                            if (cur_recv_frame & 0x3F) as u8 == MACFrame::get_frame_id(&data) {
                                if data.len() < 5 {
                                    println!("[MacController]: received NONE frame");
                                    continue;
                                } else {
                                    println!(
                                        "[MacController]: received frame id: {}",
                                        cur_recv_frame
                                    );
                                    cur_recv_frame += 1;
                                    continue_sends = 0;
                                    received.extend(MACFrame::get_payload(&data));
                                    if received.len() >= receive_byte_num {
                                        println!("received length: {} and stopped", received.len());
                                        recv_padding = false;
                                    }
                                }
                            } else {
                                println!(
                                    "[MacController]: expected frame id: {}, received id: {}",
                                    if cur_recv_frame == 0 {
                                        u8::MAX as usize
                                    } else {
                                        cur_recv_frame
                                    },
                                    MACFrame::get_frame_id(&data)
                                );
                            }
                        }
                    } else {
                        println!(
                            "[MacController]: received other macaddress: {}",
                            mac_frame::MACFrame::get_dst(&data)
                        );
                    }
                }
            }
        });
    }

    // async fn task_daemon(
    //     decoded_data_rx: &mut UnboundedReceiver<Vec<Byte>>,
    //     demodulate_status_tx: &UnboundedSender<SwitchSignal>,
    // ) {
    // }

    // true if channel empty
    // false if channel busy
    async fn send_frame(
        demodulate_status_tx: &UnboundedSender<SwitchSignal>,
        detector: &mut MacDetector,
        sender: &mut MacSender,
        to_send_frame: &MACFrame,
        to_detect: bool,
    ) -> bool {
        // demodulator close
        let _ = demodulate_status_tx.send(SwitchSignal::StopSignal);

        // send frame
        // detect
        let is_empty = if to_detect {
            detector.is_empty().await
        } else {
            true
        };
        if is_empty {
            // send the frame
            sender.send_frame(to_send_frame).await;
        }

        // demodulator open
        let _ = demodulate_status_tx.send(SwitchSignal::ResumeSignal);

        is_empty
    }
}

pub type EndSignal = bool;

pub struct MacSendTask {
    dst: MacAddress,
    to_sends: Vec<Vec<Byte>>,
    to_send_macframe: Vec<MACFrame>,
    endsignal_tx: Sender<EndSignal>,
    timer: RecordTimer,
    cur_frame: usize,
    retry_times: u64,
    resend_times: u64,
}

impl MacSendTask {
    pub fn new(dst: MacAddress, to_sends: Vec<Vec<Byte>>, signal_tx: Sender<EndSignal>) -> Self {
        Self {
            dst,
            to_sends,
            to_send_macframe: vec![],
            endsignal_tx: signal_tx,
            timer: RecordTimer::new(),
            cur_frame: 0,
            retry_times: 0,
            resend_times: 0,
        }
    }

    pub fn fresh_send_frame(&mut self, to_send_macframe: Vec<MACFrame>) {
        self.to_send_macframe = to_send_macframe;
    }
}

pub struct MacReceiveTask {
    src: MacAddress,
    decoded_tx: UnboundedSender<Vec<Byte>>,
    endsignal_rx: Receiver<EndSignal>,
    cur_frame: usize,
}

impl MacReceiveTask {
    pub fn new(
        src: MacAddress,
        decoded_tx: UnboundedSender<Vec<Byte>>,
        signal_rx: Receiver<EndSignal>,
    ) -> Self {
        Self {
            src,
            decoded_tx,
            endsignal_rx: signal_rx,
            cur_frame: 0,
        }
    }
}

pub struct MacDetector {
    request_tx: UnboundedSender<Byte>,
    result_rx: watch::Receiver<Vec<f32>>,
}

impl MacDetector {
    pub async fn new() -> (Self, UnboundedReceiver<Byte>, watch::Sender<Vec<f32>>) {
        let (request_tx, request_rx) = unbounded_channel::<Byte>();
        let (result_tx, result_rx) = watch::channel(vec![1e-5 as f32]);

        (
            Self {
                request_tx,
                result_rx,
            },
            request_rx,
            result_tx,
        )
    }

    pub async fn is_empty(&mut self) -> bool {
        let _ = self.request_tx.send(DETECT_SIGNAL);
        // println!("send request");
        // println!("channel active: {:?}", !self.result_rx.is_closed());
        // self.clear();
        // if let Some(samples) = self.result_rx.borrow() {
        // println!("data len: {}", samples.len());
        if calculate_energy((*self.result_rx.borrow()).as_slice()) < ENERGE_LIMIT {
            return true;
        }
        // }

        return false;
    }

    // fn clear(&mut self) {
    //     while self.result_rx.try_recv().is_ok() {}
    // }

    pub async fn daemon(
        mut request_rx: UnboundedReceiver<Byte>,
        result_tx: watch::Sender<Vec<f32>>,
        device: Device,
        config: SupportedStreamConfig,
    ) {
        // println!("run daemon setup");
        let mut sample_stream = InputAudioStream::new(&device, config);
        let mut sample = vec![];
        println!("detector daemon start");
        loop {
            tokio::select! {
                _ = request_rx.recv() => {
                    let _ = result_tx.send(sample_stream.next().await.unwrap());
                }

                Some(data) = sample_stream.next() =>{
                    sample = data;
                }
            }
        }
    }
}

fn calculate_energy(samples: &[f32]) -> f32 {
    let sum_of_squares: f32 = samples.iter().map(|&sample| sample * sample).sum();
    let energy = sum_of_squares / samples.len() as f32;
    // println!("avg energy: {}", energy);
    energy
}
