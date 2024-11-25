use serde::Deserialize;
use std::fs;
use std::net::Ipv4Addr;

#[derive(Deserialize, Debug, Clone)]
struct IPGenerator {
    // Phy layer
    lowest_power_limit: f32,
    // Mac layer
    mac_addr: u8,
    // IP layer
    ip_addr: Ipv4Addr,
    ip_mask: Ipv4Addr,
    ip_gateway: Ipv4Addr,
}

impl IPGenerator {
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
}
