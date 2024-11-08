use std::vec;

use crate::{
    acoustic_mac::mac_frame::{MACFrame, MACType, MacAddress},
    acoustic_modem::{
        demodulation::{Demodulation2, DemodulationState},
        generator::PhyLayerGenerator,
    },
    utils::Byte,
};
use core::result::Result::Ok;
use tokio::sync::mpsc::unbounded_channel;

pub struct MacReceiver {
    demodulator: Demodulation2,
}

impl MacReceiver {
    pub fn new(config_file: &str) -> Self {
        let config = PhyLayerGenerator::new_from_yaml(config_file);
        let demodulator = config.gen_demodulation();

        Self { demodulator }
    }

    pub async fn receive_bytes(&mut self, byte_num: usize, self_mac: MacAddress) -> Vec<Byte> {
        let (decoded_data_tx, mut decoded_data_rx) = unbounded_channel();
        let (status_tx, status_rx) = unbounded_channel();
        let listen_task = self.demodulator.listening_controlled(
            decoded_data_tx,
            status_rx,
            DemodulationState::DetectPreamble,
        );

        let recv_task = tokio::spawn(async move {
            let mut recv_data: Vec<Byte> = vec![];
            while recv_data.len() < byte_num {
                while let Some(data) = decoded_data_rx.recv().await {
                    if MACFrame::get_src(&data) == self_mac
                        && MACFrame::get_type(&data) == MACType::Data
                    {
                        recv_data.extend(data.iter());
                    }
                }
            }

            return recv_data;
        });

        let recv_data = tokio::select! {
            data = recv_task => data,
            _ = listen_task => Ok(Vec::new()),
        };

        recv_data.unwrap()
    }
}
