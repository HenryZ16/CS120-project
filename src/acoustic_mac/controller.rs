use std::{any, vec};

use crate::{
    acoustic_modem::{
        demodulation::{Demodulation2, DemodulationState, SwitchSignal, SWITCH_SIGNAL},
        generator::PhyLayerGenerator,
        modulation::Modulator,
    },
    utils::Byte,
};
use anyhow::Error;
use std::result::Result::Ok;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::unbounded_channel;

const MAX_SEND: usize = 5;

#[derive(PartialEq)]
enum ControllerState {
    Idel,
    RxFrame,
    TxACK,
    TxFrame,
    ACKTimeout,
    LinkError,
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
        receive_byte_num: usize,
        receive_output: &mut Vec<Byte>,
        send_data: &mut Vec<Byte>,
        send_byte_num: usize,
    ) -> Result<(), anyhow::Error> {
        let (decoded_data_tx, mut decoded_data_rx) = unbounded_channel();
        let (status_tx, status_rx) = unbounded_channel();

        let mut init_state = DemodulationState::DetectPreamble;
        if send_data.len() == 0 {
            init_state.switch();
        }

        // start decode listening
        let listen_task =
            self.demodulator
                .listening_controlled(decoded_data_tx, status_rx, init_state);

        let mut controller_state = ControllerState::Idel;

        // get send_frame

        // setup timer
        let mut timer = RecordTimer::new();

        let mut send_padding: bool = true;
        let mut recv_padding: bool = true;

        let mut recv_frame: Vec<Byte> = vec![];
        let mut retry_times: usize = 0;

        while send_padding || recv_padding {
            if controller_state == ControllerState::Idel {
                if let Ok(data) = decoded_data_rx.try_recv() {
                    // check data type
                }

                if timer.is_timeout() {
                    match timer.timer_type {
                        TimerType::ACK => {
                            retry_times += 1;

                            if retry_times >= MAX_SEND {
                                controller_state = ControllerState::LinkError;
                                return Err(Error::msg("link error"));
                            }

                            // send last frame again
                        }

                        TimerType::BACKOFF => {
                            // send next frame

                            timer.start(50, TimerType::ACK);
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
}
