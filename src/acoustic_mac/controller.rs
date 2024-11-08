use std::vec;

use crate::{
    acoustic_modem::{
        demodulation::{Demodulation2, DemodulationState, SwitchSignal, SWITCH_SIGNAL},
        generator::PhyLayerGenerator,
        modulation::Modulator,
    },
    utils::Byte,
};
use core::result::Result::Ok;
use tokio::sync::mpsc::unbounded_channel;

enum ControllerState {
    Idel,
    RxFrame,
    TxACK,
    TxFrame,
    ACKTimeout,
    LinkError,
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
    ) {
        let (decoded_data_tx, mut decoded_data_rx) = unbounded_channel();
        let (status_tx, status_rx) = unbounded_channel();

        let mut init_state = DemodulationState::DetectPreamble;
        if send_data.len() == 0 {
            init_state.switch();
        }

        let listen_task =
            self.demodulator
                .listening_controlled(decoded_data_tx, status_rx, init_state);
    }
}
