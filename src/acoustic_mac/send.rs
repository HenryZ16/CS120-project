use crate::{
    acoustic_mac::mac_frame,
    acoustic_mac::mac_frame::MACFrame,
    acoustic_modem::phy_frame,
    acoustic_modem::{generator::PhyLayerGenerator, modulation::Modulator},
    utils::Byte,
};
use std::vec;

const ADDRESS: u8 = 0x33;

pub struct MacSender {
    modulator: Modulator,
    address: u8,
}

impl MacSender {
    pub fn new(config_file: &str) -> Self {
        let config = PhyLayerGenerator::new_from_yaml(config_file);
        let modulator = config.gen_modulator();

        Self {
            modulator,
            address: ADDRESS,
        }
    }

    pub async fn send_frame(&mut self, frame: MACFrame) {
        let bits = frame.get_whole_frame_bits();
        self.modulator
            .send_single_ofdm_frame(bits.clone(), bits.len() as isize * 8)
            .await;
    }

    // we need modulator to determine the ofdm carrier cnt, then the length of the frame
    // so `generate_data_frames` is put here
    pub async fn generate_data_frames(&mut self, data: Vec<Byte>, dest: u8) -> Vec<MACFrame> {
        let frame_max_length =
            phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING * self.modulator.get_carrier_cnt() / 8 - 3; // 3 means dest, src, type
        let mut frames: Vec<MACFrame> = vec![];
        let mut data = data.clone();
        while !data.is_empty() {
            let payload = data.drain(0..frame_max_length).collect();
            let frame = MACFrame::new(dest, self.address, mac_frame::MACType::Data, payload);
            frames.push(frame);
        }

        frames
    }
}
