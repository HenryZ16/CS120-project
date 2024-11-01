use crate::acoustic_modem::{demodulation::Demodulation2, modulation::Modulator};
use serde::Deserialize;
use std::fs;

#[derive(Deserialize, Debug)]
pub struct PhyLayerGenerator {
    carrier_freq: Vec<u32>,
    sample_rate: u32,
    redundent_times: usize,
    enable_ofdm: bool,
}

impl PhyLayerGenerator {
    pub fn new(
        carrier_freq: Vec<u32>,
        sample_rate: u32,
        redundent_times: usize,
        enable_ofdm: bool,
    ) -> Self {
        PhyLayerGenerator {
            carrier_freq,
            sample_rate,
            redundent_times,
            enable_ofdm,
        }
    }

    pub fn new_from_yaml(filename: &str) -> Self {
        let contents = fs::read_to_string(filename).expect("Failed to read file");
        serde_yaml::from_str(&contents).expect("Failed to parse YAML")
    }

    pub fn gen_demodulation(self) -> Demodulation2 {
        Demodulation2::new(
            self.carrier_freq,
            self.sample_rate,
            "",
            self.redundent_times,
            self.enable_ofdm,
            0,
            0,
        )
    }

    pub fn gen_modulator(self) -> Modulator {
        Modulator::new(self.carrier_freq, self.sample_rate, self.enable_ofdm)
    }
}
