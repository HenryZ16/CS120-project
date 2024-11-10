use crate::acoustic_modem::generator::PhyLayerGenerator;

const TEST_FILE: &str = "configuration/pa2.yml";

#[test]
pub fn test_config_new() {
    println!("{:?}", PhyLayerGenerator::new_from_yaml(TEST_FILE));
}

#[test]
pub fn test_demodulation() {
    let yaml_config = PhyLayerGenerator::new_from_yaml(TEST_FILE);
    let (device, config) = crate::utils::get_audio_device_and_config(yaml_config.get_sample_rate());
    let _demodulation = yaml_config.gen_demodulation(device, config);
}

#[test]
pub fn test_modulator() {
    let yaml_config = PhyLayerGenerator::new_from_yaml(TEST_FILE);
    let (device, config) = crate::utils::get_audio_device_and_config(yaml_config.get_sample_rate());
    let mut modulator = yaml_config.gen_modulator(device, config);
    let _ = modulator.test_carrier_wave();
}
