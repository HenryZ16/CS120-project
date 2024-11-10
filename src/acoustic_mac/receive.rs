use std::{time::Duration, vec};

use crate::{
    acoustic_mac::mac_frame::{MACFrame, MACType, MacAddress},
    acoustic_modem::{
        demodulation::{Demodulation2, DemodulationState, SwitchSignal},
        generator::PhyLayerGenerator,
    },
    utils::Byte,
};
use core::result::Result::Ok;
use tokio::sync::mpsc::unbounded_channel;
use tokio_util::io::ReaderStream;

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
        let listen_task = self.demodulator.listening_daemon(
            decoded_data_tx,
            status_rx,
            DemodulationState::DetectPreamble,
        );
        println!("receive task start");
        // let _ = tokio::spawn(async move {
        //     loop {
        //         tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        //         println!("send signal");
        //         let _ = status_tx.send(SWITCH_SIGNAL);
        //     }
        // });
        let recv_task = tokio::spawn(async move {
            let mut recv_data: Vec<Byte> = vec![];
            // let mut decoded_data_stream = UnboundedReceiverStream::new(decoded_data_rx);
            while recv_data.len() < byte_num {
                while let Some(data) = decoded_data_rx.recv().await {
                    // println!("received raw data: {:?}", data);
                    if MACFrame::get_dst(&data) == self_mac
                        && MACFrame::get_type(&data) == MACType::Data
                    {
                        println!("receive mac frame");
                        recv_data.extend_from_slice(MACFrame::get_payload(&data));
                    } else {
                        println!("receive wrong dst: {}", MACFrame::get_dst(&data));
                    }
                }
                // tokio::time::sleep(Duration::from_millis(500)).await;
                // println!("channel size: {}", decoded_data_stream.as_ref().len());
            }

            println!("stoped");
            return recv_data;
        });

        let recv_data = tokio::select! {
            data = recv_task => data,
            _ = listen_task => Ok(Vec::new()),
        };

        println!("select down");
        recv_data.unwrap()
    }
}
