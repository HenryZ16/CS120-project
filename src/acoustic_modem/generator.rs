use crate::acoustic_modem::{demodulation::Demodulation2, modulation::Modulator};
use serde::Deserialize;
use std::fs;

#[derive(Deserialize, Debug, Clone)]
pub struct PhyLayerGenerator {
    // phy_frame parameters
    max_frame_data_length: usize,
    frame_payload_length: usize,
    // frame_length_length: usize,
    max_frame_data_length_no_encoding: usize,
    frame_length_length_no_encoding: usize,
    frame_crc_length_no_encoding: usize,

    // modulator parameters
    // frame_distance: usize,

    // demodulation parameters
    lowest_power_limit: f32,

    // common parameters
    carrier_freq: Vec<u32>,
    sample_rate: u32,
    redundent_times: usize,
    enable_ofdm: bool,
    enable_ecc: bool,
    #[serde(skip)]
    payload_bits_length: usize,
    #[serde(skip)]
    data_bits_length: usize,
}

impl PhyLayerGenerator {
    pub fn new_from_yaml(filename: &str) -> Self {
        let contents = fs::read_to_string(filename).expect("Failed to read file");
        let mut config: Self = serde_yaml::from_str(&contents).expect("Failed to parse YAML");

        if config.enable_ecc {
            config.payload_bits_length = config.frame_payload_length;
            config.data_bits_length = config.max_frame_data_length;
        } else {
            config.payload_bits_length = config.frame_crc_length_no_encoding
                + config.frame_length_length_no_encoding
                + config.max_frame_data_length_no_encoding;
            config.data_bits_length = config.max_frame_data_length_no_encoding;
        }

        config
    }

    pub fn get_sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn gen_demodulation(
        &self,
        device: cpal::Device,
        config: cpal::SupportedStreamConfig,
    ) -> Demodulation2 {
        Demodulation2::new_with_device_config(
            self.carrier_freq.clone(),
            self.sample_rate,
            self.redundent_times,
            self.enable_ofdm,
            self.payload_bits_length,
            self.data_bits_length,
            self.lowest_power_limit,
            device,
            config,
        )
    }

    pub fn gen_modulator(
        &self,
        device: cpal::Device,
        config: cpal::SupportedStreamConfig,
    ) -> Modulator {
        Modulator::new(self.carrier_freq.clone(), device, config, self.enable_ofdm)
    }
}
