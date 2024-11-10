use cpal::{Device, SupportedStreamConfig};

use crate::{
    acoustic_mac::mac_frame::{self, MACFrame},
    acoustic_modem::{
        generator::{self, PhyLayerGenerator},
        modulation::Modulator,
        phy_frame,
    },
    utils::Byte,
};
use std::vec;

pub struct MacSender {
    modulator: Modulator,
    address: u8,
}

impl MacSender {
    pub fn new(config_file: &str, address: u8) -> Self {
        let config = PhyLayerGenerator::new_from_yaml(config_file);
        let (cpal_device, cpal_config) =
            crate::utils::get_audio_device_and_config(config.get_sample_rate());
        let modulator = config.gen_modulator(cpal_device, cpal_config);

        Self { modulator, address }
    }

    pub fn new_from_genrator(
        generator: &PhyLayerGenerator,
        address: u8,
        device: Device,
        config: SupportedStreamConfig,
    ) -> Self {
        let modulator = generator.gen_modulator(device, config);

        Self { modulator, address }
    }

    // for debug use
    pub async fn send_modulated_signal(&mut self, data: Vec<f32>) {
        self.modulator.send_modulated_signal(data).await;
    }

    pub async fn send_frame(&mut self, frame: &MACFrame) {
        let bits = frame.get_whole_frame_bits();
        // println!("[MacSender::send_frame] bits: {:?}", bits);
        self.modulator
            .send_single_ofdm_frame(bits.clone(), bits.len() as isize * 8)
            .await;
    }

    pub fn generate_ack_frame(&mut self, dest: u8) -> MACFrame {
        MACFrame::new(dest, self.address, mac_frame::MACType::Ack, vec![])
    }

    // we need modulator to determine the ofdm carrier cnt, then the length of the frame
    // so `generate_data_frames` is put here
    pub fn generate_data_frames(&mut self, data: Vec<Byte>, dest: u8) -> Vec<MACFrame> {
        let frame_max_length =
            phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING * self.modulator.get_carrier_cnt() / 8 - 3; // 3 means dest, src, type
        let mut frames: Vec<MACFrame> = vec![];
        let mut data = data.clone();
        while data.len() > frame_max_length {
            let payload: Vec<u8> = data.drain(0..frame_max_length).collect();
            let frame = MACFrame::new(dest, self.address, mac_frame::MACType::Data, payload);
            frames.push(frame);
        }
        if !data.is_empty() {
            let frame = MACFrame::new(dest, self.address, mac_frame::MACType::Data, data);
            frames.push(frame);
        }

        frames
    }
}
