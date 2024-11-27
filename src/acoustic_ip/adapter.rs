use crate::acoustic_ip::ip_packet::IpProtocol;
use crate::acoustic_ip::protocols::icmp::{ICMPType, ICMP};
use crate::acoustic_mac::net_card::NetCard;
use crate::utils::Byte;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::thread::JoinHandle;
use wintun::{Packet, Session};

use super::ip_packet::IpPacket;

pub struct Adapter {
    // to IP layer
    ip_addr: Ipv4Addr,
    ip_mask: Ipv4Addr,
    ip_gateway: Option<Ipv4Addr>,
    if_router: bool,
    session: Arc<Session>,
    if_static_arp: bool,
    arp_table: HashMap<Ipv4Addr, u8>,
    // to MAC layer
    mac_address: u8,
    net_card: NetCard,
}

impl Adapter {
    pub fn new(
        ip_addr: Ipv4Addr,
        ip_mask: Ipv4Addr,
        ip_gateway: Option<Ipv4Addr>,
        if_router: bool,
        if_static_arp: bool,
        arp_table: HashMap<Ipv4Addr, u8>,
        mac_address: u8,
        net_card: NetCard,
    ) -> Self {
        let ip_gateway = if if_router { None } else { ip_gateway };

        let wintun = unsafe { wintun::load_from_path("external\\wintun\\bin\\amd64\\wintun.dll") }
            .expect("Failed to load wintun dll");
        let adapter = match wintun::Adapter::open(&wintun, "AcousticNet") {
            Ok(adapter) => adapter,
            Err(_) => wintun::Adapter::create(
                &wintun,
                "AcousticNet",
                "Wintun",
                Some(0x12344321123443211234432112344321),
            )
            .expect("Failed to create wintun adapter!"),
        };
        println!("Adapter name: {:?}", adapter.get_name());
        adapter.set_address(ip_addr).unwrap();
        adapter.set_netmask(ip_mask).unwrap();
        adapter.set_gateway(ip_gateway).unwrap();
        let session = Arc::new(adapter.start_session(wintun::MAX_RING_CAPACITY).unwrap());

        let arp_table = if if_static_arp {
            arp_table
        } else {
            HashMap::new()
        };

        Self {
            ip_addr,
            ip_mask,
            ip_gateway,
            if_router,
            session,
            if_static_arp,
            arp_table,
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
            config.get_if_router(),
            config.get_if_static_arp(),
            config.get_arp_table(),
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

    async fn up_daemon(&mut self) {
        match self.net_card.try_recv() {
            Ok(data) => {
                // println!("[up_daemon]: received from mac layer");
                if data[0] >> 4 != 4 {
                    return;
                }

                let packet = IpPacket::new_from_bytes(&data);
                // u32::MAX: broadcast addr
                if !self.if_router
                    && (packet.get_destination_address() != self.ip_addr.to_bits()
                        && packet.get_destination_address() != u32::MAX)
                {
                    return;
                }
                match packet.get_protocol() {
                    IpProtocol::ICMP => {
                        if !packet.check_header_checksum() {
                            return;
                        }
                        let icmp = ICMP::try_new_from_ip_packet(&packet).unwrap();
                        if !icmp.check_checksum() {
                            return;
                        }
                        println!(
                            "Receive ICMP Echo packet from {:?} to {:?}",
                            Ipv4Addr::from(packet.get_source_address()),
                            Ipv4Addr::from(packet.get_destination_address())
                        );
                        if icmp.get_type() != ICMPType::EchoRequest
                            || (packet.get_destination_address() != self.ip_addr.to_bits()
                                && packet.get_destination_address() != u32::MAX)
                        {
                            println!("Forwarding ICMP packet");
                            self.send_to_ip(packet);
                            return;
                        }

                        // println!(
                        //     "Received ICMP Echo Request from {:?}",
                        //     Ipv4Addr::from_bits(packet.get_source_address())
                        // );

                        let reply = icmp.reply_echo();
                        let mut reply_packet = packet.clone();
                        reply_packet.set_destination_address(packet.get_source_address());
                        reply_packet.set_source_address(packet.get_destination_address());
                        reply_packet.set_data(reply.get_icmp_bytes());

                        let dst_mac = match self
                            .arp_table
                            .get(&Ipv4Addr::from_bits(packet.get_source_address()))
                        {
                            Some(mac) => *mac,
                            None => u8::MAX,
                        };

                        let _ = self
                            .net_card
                            .send_unblocked(dst_mac, reply_packet.get_ip_packet_bytes());
                    }
                    _ => {
                        self.send_to_ip(packet);
                    }
                }
            }
            Err(_) => {}
        }
    }

    async fn down_daemon(&mut self) {
        match self.receive_from_ip_async() {
            Ok(packet) => {
                if packet.dst_is_subnet(&self.ip_addr, &self.ip_mask)
                    || packet.get_destination_address() == u32::MAX
                {
                    if let Some(&dst_mac) = self.arp_table.get(&packet.get_destination_ipv4_addr())
                    {
                        let _ = self
                            .net_card
                            .send_unblocked(dst_mac, packet.get_ip_packet_bytes());
                    }
                } else if !self.if_router {
                    let _ = self
                        .net_card
                        .send_async(
                            *self
                                .arp_table
                                .get(&self.ip_gateway.unwrap())
                                .expect("No gateway"),
                            packet.get_ip_packet_bytes(),
                        )
                        .await;
                }
            }
            Err(_) => {}
        }
    }

    pub async fn adapter_daemon(&mut self) {
        // 1. Listen from the mac layer (up)
        //    if `ping` echoRequest, send `ping` echoReply
        //    else, send the packet to the ip layer
        // 2. Listen from the ip layer (down)
        //    if the dest is directly connected or broadcast
        //    then send the packet to the mac layer
        //    else if the dest is not directly connected
        //    we may need to send the packet to the gateway
        //    then send the packet to the mac layer
        //  - both up and down should work concurrently
        println!("Adapter Daemon started.");
        loop {
            self.up_daemon().await;
            self.down_daemon().await;
            let _ = tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
        }
    }

    pub async fn start_daemon(mut adapter: Adapter) -> tokio::task::JoinHandle<()> {
        let main_task = tokio::spawn(async move {
            adapter.adapter_daemon().await;
        });
        main_task
    }
    pub async fn stop_daemon(adapter_task: tokio::task::JoinHandle<()>) {
        adapter_task.abort();
    }
}
