use crate::acoustic_mac::net_card::NetCard;
use crate::utils::Byte;
use std::net::Ipv4Addr;
use std::sync::Arc;
use wintun::{Packet, Session};

use super::ip_packet::IpPacket;

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
            config.get_net_card(),
        )
    }

    pub fn send_to_ip(&self, packet: IpPacket) {
        let packet_bytes = packet.get_ip_packet_bytes();
        let mut return_packet = self
            .session
            .allocate_send_packet(packet_bytes.len() as u16)
            .unwrap();
        let return_bytes = return_packet.bytes_mut();
        return_bytes.copy_from_slice(&packet_bytes);
        self.session.send_packet(return_packet);
    }

    pub fn receive_from_ip_blocking(&self) -> Result<IpPacket, &'static str> {
        let packet = self.session.receive_blocking().unwrap();
        let packet_bytes = packet.bytes().to_vec();
        if packet_bytes[0] >> 4 != 4 {
            Err("Only IPv4 is supported")?;
        }
        Ok(IpPacket::new_from_bytes(&packet_bytes))
    }

    pub fn receive_from_ip_async(&self) -> Result<IpPacket, &'static str> {
        match self.session.try_receive() {
            Ok(Some(packet)) => {
                let packet_bytes = packet.bytes().to_vec();
                if packet_bytes[0] >> 4 != 4 {
                    Err("Only IPv4 is supported")?;
                }
                Ok(IpPacket::new_from_bytes(&packet_bytes))
            }
            Ok(None) => Err("No packet received")?,
            Err(_) => Err("Failed to receive packet")?,
        }
    }

    async fn up_daemon(&self) {}
    async fn down_daemon(&self) {}

    pub async fn adapter_daemon(&self) {
        // 1. Listen from the mac layer (up)
        //    if `ping` echoRequest, send `ping` echoReply
        //    else, send the packet to the ip layer
        // 2. Listen from the ip layer (down)
        //    if is router, and the subnet of the packet is for the acoustic network,
        //    send the packet to the mac layer
        //    else, do nothing
    }

    pub async fn start_daemon(&self) {}
    pub async fn stop_daemon(&self) {}
}
