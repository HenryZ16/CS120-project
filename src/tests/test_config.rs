use std::vec;

use crate::acoustic_modem::generator::PhyLayerGenerator;

const TEST_FILE: &str = "configuration/pa2.yml";

#[test]
pub fn test_config_new() {
    println!("{:?}", PhyLayerGenerator::new_from_yaml(TEST_FILE));
}

#[test]
pub fn test_demodulation() {
    let config = PhyLayerGenerator::new_from_yaml(TEST_FILE);
    let mut demodulation = config.gen_demodulation();
}

#[test]
pub fn test_modulator() {
    let config = PhyLayerGenerator::new_from_yaml(TEST_FILE);
    let mut modulator = config.gen_modulator();
    modulator.test_carrier_wave();
}
