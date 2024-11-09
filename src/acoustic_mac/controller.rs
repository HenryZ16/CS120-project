use std::{mem, vec};

use crate::{
    acoustic_mac::mac_frame,
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
use std::result::Result::Ok;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use super::mac_frame::MACType;

const MAX_SEND: usize = 5;

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
}

impl RecordTimer {
    fn new() -> Self {
        let start_instant = Instant::now();

        Self {
            start_instant,
            duration: Duration::new(0, 0),
            timer_type: TimerType::None,
        }
    }

    // arg: duration in ms
    fn start(&mut self, duration: u64, timer_type: TimerType) {
        self.start_instant = Instant::now();
        self.duration = Duration::from_millis(duration);
        self.timer_type = timer_type;
    }

    fn is_timeout(&self) -> bool {
        self.start_instant.elapsed() > self.duration
    }
}

struct MacController {
    modulator: Modulator,
    demodulator: Demodulation2,
}

impl MacController {
    fn new(config_file: &str) -> Self {
        let config = PhyLayerGenerator::new_from_yaml(config_file);
        let demodulator = config.gen_demodulation();
        let modulator = config.gen_modulator();

        Self {
            modulator,
            demodulator,
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
        let mut detector = MacDetector::new().await;

        // start decode listening
        let _listen_task = self.demodulator.listening_controlled(
            decoded_data_tx,
            demodulate_status_rx,
            init_state,
        );

        let mut controller_state = ControllerState::Idel;

        // get send_frame

        // setup timer
        let mut timer = RecordTimer::new();

        let mut send_padding: bool = true;
        let mut recv_padding: bool = true;

        // let mut recv_frame: Vec<Byte> = vec![];
        let mut retry_times: usize = 0;

        while send_padding || recv_padding {
            if controller_state == ControllerState::Idel {
                if let Ok(data) = decoded_data_rx.try_recv() {
                    // check data type
                    if mac_frame::MACFrame::get_dst(&data) == 1 {
                        if mac_frame::MACFrame::get_type(&data) == MACType::Ack {
                            println!("received ack");

                            // whether still have send task

                            // set backoff timer
                            timer.start(50, TimerType::BACKOFF);
                        } else {
                            println!("received data");
                            receive_output
                                .extend_from_slice(mac_frame::MACFrame::get_payload(&data));
                            if receive_output.len() >= receive_byte_num {
                                recv_padding = false;
                            }

                            // send ack
                            Self::send_frame(&demodulate_status_tx, &mut detector, false).await;
                        }
                    } else {
                        println!(
                            "received other macaddress: {}",
                            mac_frame::MACFrame::get_dst(&data)
                        );
                    }
                }

                if timer.is_timeout() {
                    match timer.timer_type {
                        TimerType::ACK => {
                            retry_times += 1;

                            if retry_times >= MAX_SEND {
                                // controller_state = ControllerState::LinkError;
                                return Err(Error::msg("link error"));
                            }

                            // set timer
                            timer.start(50, TimerType::BACKOFF);
                        }

                        TimerType::BACKOFF => {
                            // send next frame
                            if Self::send_frame(&demodulate_status_tx, &mut detector, true).await {
                                // if success, set ack timer
                                timer.start(50, TimerType::ACK);
                            } else {
                                timer.start(50, TimerType::BACKOFF);
                            }
                        }

                        _ => {}
                    }
                }
            }

            if controller_state == ControllerState::TxACK {
                // send ack

                controller_state = ControllerState::Idel;
                continue;
            }
        }

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
const ENERGE_LIMIT: f32 = 4.0;
pub struct MacDetector {
    request_tx: UnboundedSender<Byte>,
    result_rx: UnboundedReceiver<Vec<f32>>,
}

impl MacDetector {
    pub async fn new() -> Self {
        let (request_tx, request_rx) = unbounded_channel::<Byte>();
        let (result_tx, result_rx) = unbounded_channel();

        // tokio::spawn(move Self::daemon(request_rx, result_tx.clone()));

        Self {
            request_tx,
            result_rx,
        }
    }

    pub async fn is_empty(&mut self) -> bool {
        let _ = self.request_tx.send(DETECT_SIGNAL);
        println!("send request");
        // println!("channel active: {:?}", self.result_rx.is_closed());
        if let Some(samples) = self.result_rx.recv().await {
            println!("received data");
            if calculate_energy(&samples) < ENERGE_LIMIT {
                return true;
            }
        }

        return false;
    }

    async fn daemon(mut request_rx: UnboundedReceiver<Byte>, result_tx: UnboundedSender<Vec<f32>>) {
        // tokio::spawn(async)
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
        loop {
            tokio::select! {
                _ = request_rx.recv() => {
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
    energy
}
