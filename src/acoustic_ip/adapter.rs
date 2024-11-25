use crate::acoustic_mac::net_card::NetCard;
use crate::utils::Byte;
use std::net::Ipv4Addr;
use std::sync::Arc;
use wintun::{Packet, Session};

pub struct Adapter {
    // to IP layer
    ip_addr: Ipv4Addr,
    ip_mask: Ipv4Addr,
    ip_gateway: Option<Ipv4Addr>,
    session: Arc<Session>,
    // to MAC layer
    mac_address: u8,
    net_card: NetCard,
}

impl Adapter {
    pub fn new(
        ip_addr: Ipv4Addr,
        ip_mask: Ipv4Addr,
        ip_gateway: Option<Ipv4Addr>,
        mac_address: u8,
        net_card: NetCard,
    ) -> Self {
        let wintun = unsafe { wintun::load_from_path("external\\wintun\\bin\\amd64\\wintun.dll") }
            .expect("Failed to load wintun dll");
        let adapter = match wintun::Adapter::open(&wintun, "AcousticNet") {
            Ok(adapter) => adapter,
            Err(_) => wintun::Adapter::create(&wintun, "AcousticNet", "Wintun", None)
                .expect("Failed to create wintun adapter!"),
        };
        adapter.set_address(ip_addr).unwrap();
        adapter.set_netmask(ip_mask).unwrap();
        adapter.set_gateway(ip_gateway).unwrap();
        let session = Arc::new(adapter.start_session(wintun::MAX_RING_CAPACITY).unwrap());

        Self {
            ip_addr,
            ip_mask,
            ip_gateway,
            session,
            mac_address,
            net_card,
        }
    }

    pub fn new_from_config(config_file: &str) -> Self {
        let config = crate::generator::ConfigGenerator::new_from_yaml(config_file);
        Self::new(
            config.get_ip_addr(),
            config.get_ip_mask(),
            Some(config.get_ip_gateway()),
            config.get_mac_addr(),
            NetCard::new(config.get_lowest_power_limit()),
        )
    }
}
