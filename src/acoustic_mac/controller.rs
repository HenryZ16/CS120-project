use std::{mem, vec};

use crate::{
    acoustic_mac::mac_frame::{self, MACFrame},
    acoustic_modem::{
        demodulation::{Demodulation2, DemodulationState, SwitchSignal, SWITCH_SIGNAL},
        generator::PhyLayerGenerator,
        modulation::Modulator,
    },
    asio_stream::InputAudioStream,
    utils::Byte,
};
use anyhow::Error;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{SampleRate, SupportedStreamConfig};
use futures::StreamExt;
use serde::de::value::BytesDeserializer;
use std::result::Result::Ok;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use super::mac_frame::{MACType, MacAddress};
use rand::{rngs::StdRng, Rng, SeedableRng};

const MAX_SEND: u64 = 5;
const ACK_WAIT_TIME: u64 = 80 * 2;
const BACKOFF_SLOT_TIME: u64 = 90;

#[derive(PartialEq)]
enum ControllerState {
    Idel,
    TxACK,
    TxFrame,
    ACKTimeout,
}

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
            duration: Duration::new(0, 0),
            timer_type: TimerType::None,
            rng,
        }
    }

    // arg: duration in ms
    fn start(&mut self, timer_type: TimerType, factor: u64) {
        self.start_instant = Instant::now();

        self.duration = match timer_type {
            TimerType::BACKOFF => {
                let slot_times: u64 = self.rng.gen_range(0..=factor);
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

struct MacController {
    phy_config: PhyLayerGenerator,
    mac_address: MacAddress,
}

impl MacController {
    fn new(config_file: &str, mac_address: MacAddress) -> Self {
        let phy_config = PhyLayerGenerator::new_from_yaml(config_file);

        Self {
            phy_config,
            mac_address,
        }
    }

    async fn task(
        &mut self,
        receive_output: &mut Vec<Byte>,
        receive_byte_num: usize,
        send_data: &mut Vec<Byte>,
        send_byte_num: usize,
    ) -> Result<(), anyhow::Error> {
        let (decoded_data_tx, mut decoded_data_rx) = unbounded_channel();
        let (demodulate_status_tx, demodulate_status_rx) = unbounded_channel();

        let init_state = DemodulationState::DetectPreamble;
        let (mut detector, request_rx, result_tx) = MacDetector::new().await;
        let mut demodulator = self.phy_config.gen_demodulation();
        // start decode listening
        let _listen_task =
            demodulator.listening_daemon(decoded_data_tx, demodulate_status_rx, init_state);
        let _detector_daemon = MacDetector::daemon(request_rx, result_tx);

        let mut controller_state = ControllerState::Idel;

        // get send_frame

        // setup timer
        let mut timer = RecordTimer::new();

        let mut send_padding: bool = true;
        let mut recv_padding: bool = true;
        let mac_address = self.mac_address;

        let main_task = tokio::spawn(async move {
            let mut received: Vec<Byte> = vec![];
            let mut retry_times: u64 = 0;
            while send_padding || recv_padding {
                if controller_state == ControllerState::Idel {
                    if let Ok(data) = decoded_data_rx.try_recv() {
                        // check data type
                        if mac_frame::MACFrame::get_dst(&data) == mac_address {
                            if mac_frame::MACFrame::get_type(&data) == MACType::Ack {
                                println!("received ack");
                                retry_times = 0;
                            } else {
                                println!("received data");

                                received.extend_from_slice(MACFrame::get_payload(&data));
                                if received.len() >= receive_byte_num {
                                    recv_padding = false;
                                }

                                Self::send_frame(&demodulate_status_tx, &mut detector, false).await;
                            }
                        } else {
                            println!(
                                "received other macaddress: {}",
                                mac_frame::MACFrame::get_dst(&data)
                            );
                        }
                    }
                }

                if timer.is_timeout() {
                    match timer.timer_type {
                        TimerType::ACK => {
                            retry_times += 1;

                            if retry_times >= MAX_SEND {
                                return Err(Error::msg("link error"));
                            }

                            timer.start(TimerType::BACKOFF, retry_times);
                        }
                        TimerType::BACKOFF => {
                            if MacController::send_frame(&demodulate_status_tx, &mut detector, true)
                                .await
                            {
                                timer.start(TimerType::ACK, retry_times);
                            } else {
                                timer.start(TimerType::BACKOFF, retry_times);
                            }
                        }
                        _ => {}
                    }

                    if controller_state == ControllerState::TxACK {
                        controller_state = ControllerState::Idel;
                        MacController::send_frame(&demodulate_status_tx, &mut detector, false)
                            .await;
                    }
                }
            }

            return Ok(received);
        });

        let handle = tokio::select! {
            _ = _listen_task => {Ok(vec![])}
            _ = _detector_daemon => {Ok(vec![])}
            data = main_task => {
                if let Ok(data) = data{
                    data
                }
                else
                {
                    Ok(vec![])
                }
            }
        };
        receive_output.extend(handle.unwrap().iter());
        Ok(())
    }

    // true if channel empty
    // false if channel busy
    async fn send_frame(
        demodulate_status_tx: &UnboundedSender<SwitchSignal>,
        detector: &mut MacDetector,
        to_detect: bool,
    ) -> bool {
        // demodulator close
        let _ = demodulate_status_tx.send(SWITCH_SIGNAL);

        // send frame
        // detect
        let is_empty = if to_detect {
            detector.is_empty().await
        } else {
            true
        };
        if is_empty {
            // send the frame
        }

        // demodulator open
        let _ = demodulate_status_tx.send(SWITCH_SIGNAL);

        is_empty
    }
}

const DETECT_SIGNAL: Byte = 1;
const ENERGE_LIMIT: f32 = 0.001;
pub struct MacDetector {
    request_tx: UnboundedSender<Byte>,
    result_rx: UnboundedReceiver<Vec<f32>>,
}

impl MacDetector {
    pub async fn new() -> (Self, UnboundedReceiver<Byte>, UnboundedSender<Vec<f32>>) {
        let (request_tx, request_rx) = unbounded_channel::<Byte>();
        let (result_tx, result_rx) = unbounded_channel();

        // tokio::spawn(move Self::daemon(request_rx, result_tx.clone()));

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
        self.clear();
        if let Some(samples) = self.result_rx.recv().await {
            // println!("data len: {}", samples.len());
            if calculate_energy(&samples) < ENERGE_LIMIT {
                return true;
            }
        }

        return false;
    }

    fn clear(&mut self) {
        while self.result_rx.try_recv().is_ok() {}
    }

    pub async fn daemon(
        mut request_rx: UnboundedReceiver<Byte>,
        result_tx: UnboundedSender<Vec<f32>>,
    ) {
        // println!("run daemon setup");
        let host = cpal::host_from_id(cpal::HostId::Asio).expect("failed to initialise ASIO host");
        // let host = cpal::default_host();
        let device = host.input_devices().expect("failed to find input device");
        let device = device
            .into_iter()
            .next()
            .expect("no input device available");

        let default_config = device.default_input_config().unwrap();
        let config = SupportedStreamConfig::new(
            // default_config.channels(),
            1,                 // mono
            SampleRate(48000), // sample rate
            default_config.buffer_size().clone(),
            default_config.sample_format(),
        );

        let mut sample_stream = InputAudioStream::new(&device, config);
        let mut sample = vec![];
        println!("daemon stream set up");
        loop {
            tokio::select! {
                _ = request_rx.recv() => {
                    if sample.len() == 0{
                        sample = sample_stream.next().await.unwrap();
                    }
                    let _ = result_tx.send(mem::replace(&mut sample, vec![]));
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
    println!("avg energy: {}", energy);
    energy
}
