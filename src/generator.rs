use cpal::{Device, SupportedStreamConfig};
use serde::Deserialize;
use std::fs;
use std::net::Ipv4Addr;

use crate::acoustic_mac::net_card::NetCard;
use crate::acoustic_modem::demodulation::Demodulation2;
use crate::acoustic_modem::modulation::Modulator;

#[derive(Deserialize, Debug, Clone)]
pub struct ConfigGenerator {
    // Phy layer
    lowest_power_limit: f32,
    sample_rate: u32,
    // Mac layer
    mac_addr: u8,
    // IP layer
    ip_addr: Ipv4Addr,
    ip_mask: Ipv4Addr,
    ip_gateway: Ipv4Addr,
}

impl ConfigGenerator {
    pub fn new_from_yaml(filename: &str) -> Self {
        let contents = fs::read_to_string(filename).expect("Failed to read file");
        let config: Self = serde_yaml::from_str(&contents).expect("Failed to parse YAML");
        config
    }

    pub fn get_lowest_power_limit(&self) -> f32 {
        self.lowest_power_limit
    }
    pub fn get_mac_addr(&self) -> u8 {
        self.mac_addr
    }
    pub fn get_ip_addr(&self) -> Ipv4Addr {
        self.ip_addr
    }
    pub fn get_ip_mask(&self) -> Ipv4Addr {
        self.ip_mask
    }
    pub fn get_ip_gateway(&self) -> Ipv4Addr {
        self.ip_gateway
    }
    pub fn get_sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn get_audio_device_and_config(&self) -> (cpal::Device, cpal::SupportedStreamConfig) {
        use cpal::traits::{DeviceTrait, HostTrait};
        use cpal::{SampleRate, SupportedStreamConfig};

        let host = cpal::host_from_id(cpal::HostId::Asio).expect("failed to initialise ASIO host");
        // let host = cpal::default_host();
        let device = host.default_output_device().unwrap();
        println!(
            "[get_audio_device_and_config] Output device: {:?}",
            device.name().unwrap()
        );

        let default_config = device.default_output_config().unwrap();
        let config = SupportedStreamConfig::new(
            1,                            // mono
            SampleRate(self.sample_rate), // sample rate
            default_config.buffer_size().clone(),
            default_config.sample_format(),
        );
        println!("[get_audio_device_and_config] Output config: {:?}", config);
        return (device, config);
    }

    pub fn get_modulator(&self, device: Device, config: SupportedStreamConfig) -> Modulator {
        Modulator::new_from_config(device, config)
    }

    pub fn get_demodulator(&self, device: Device, config: SupportedStreamConfig) -> Demodulation2 {
        Demodulation2::new_from_config(self.sample_rate, self.lowest_power_limit, device, config)
    }

    pub fn get_net_card(&self) -> NetCard {
        NetCard::new_from_config(self)
    }
}
