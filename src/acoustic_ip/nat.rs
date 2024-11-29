use pnet::datalink::Channel::Ethernet;
use pnet::datalink::{self, NetworkInterface};
use pnet::packet::ethernet::{EtherTypes, EthernetPacket, MutableEthernetPacket};
use pnet::packet::icmp::{IcmpPacket, IcmpType};
use pnet::packet::ipv4::Ipv4Packet;
use pnet::packet::{MutablePacket, Packet};

use pnet::packet::{
    icmp::{
        echo_reply::EchoReplyPacket,
        echo_request::{IcmpCodes, MutableEchoRequestPacket},
        IcmpTypes,
    },
    ip::IpNextHeaderProtocols,
    util,
};
use pnet::transport::icmp_packet_iter;
use pnet::util::checksum;
use pnet_transport::TransportChannelType::{Layer3, Layer4};
use pnet_transport::{transport_channel, TransportProtocol, TransportSender};
use rand::random;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Mutex;
use std::thread::{self, panicking};
use std::{
    env,
    net::IpAddr,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use super::adapter::Adapter;
use super::ip_packet::{IpPacket, IpProtocol};
use super::protocols::icmp::{ICMPType, ICMP};

// const ICMP_HEADER_SIZE: usize = 64;

// pub fn nat_listen_daemon(if_index: ) {
//     let protocol = Layer4(TransportProtocol::Ipv4(IpNextHeaderProtocols::Icmp));
//     let (mut tx, mut rx) = transport_channel(4096, protocol).unwrap();

//     let mut iter = icmp_packet_iter(&mut rx);
//     while let Ok(data) = iter.next() {

//     }
// }

pub fn nat_forward_daemon(
    mut forward_acoustic_rx: UnboundedReceiver<IpPacket>,
    forward_acoustic_tx: UnboundedSender<IpPacket>,
    additonal_if: Vec<u32>,
    acoustic_domain: Ipv4Addr,
    acoustic_mask: Ipv4Addr,
) {
    let nat_table = Arc::new(Mutex::new(HashMap::new()));

    let nat_table_clone = Arc::clone(&nat_table);
    thread::spawn(move || {
        let protocol = Layer4(TransportProtocol::Ipv4(IpNextHeaderProtocols::Icmp));
        let (mut icmp_tx, mut icmp_rx) = transport_channel(128, protocol).unwrap();

        while let Some(packet) = forward_acoustic_rx.blocking_recv() {
            // println!("received packets");
            match packet.get_protocol() {
                IpProtocol::ICMP => {
                    let mut icmp_header = vec![0; packet.get_internet_header_length() as usize];
                    let new_packet = ip_packet_to_icmp(&packet, &mut icmp_header);
                    // println!("icmp dst: {:?}", packet.get_destination_ipv4_addr());
                    let mut nat_table_handle = nat_table_clone.lock().unwrap();
                    nat_table_handle
                        .insert(new_packet.get_identifier(), packet.get_source_address());
                    println!("new packet: {:?}", new_packet.payload());
                    let _ = icmp_tx.send_to(new_packet, packet.get_destination_ipv4_addr().into());
                }
                _ => {}
            }
        }
    });

    // let protocol = Layer3()

    for interface_index in additonal_if {
        let forward_acoustic_tx_copy = forward_acoustic_tx.clone();

        let interfaces = datalink::interfaces();
        let interface = interfaces
            .into_iter()
            .filter(|ifaces: &NetworkInterface| ifaces.index == interface_index)
            .next()
            .expect("Error getting interface");

        let (tx, mut rx) = match datalink::channel(&interface, Default::default()) {
            Ok(Ethernet(tx, rx)) => (tx, rx),
            Ok(_) => panic!("Unhandled channel type"),
            Err(e) => panic!(
                "An error occurred when creating the datalink channel: {}",
                e
            ),
        };

        let nat_table_clone = Arc::clone(&nat_table);
        thread::spawn(move || loop {
            match rx.next() {
                Ok(packet) => {
                    let packet = EthernetPacket::new(packet).unwrap();
                    match packet.get_ethertype() {
                        EtherTypes::Ipv4 => {
                            let header = Ipv4Packet::new(packet.payload()).unwrap();
                            let mut packet = IpPacket::new_from_bytes(packet.payload());

                            match header.get_next_level_protocol() {
                                IpNextHeaderProtocols::Icmp => {
                                    // println!(
                                    //     "receive acoustic packet: {:?}, length: {}, head length: {}",
                                    //     packet,
                                    //     packet.get_total_length(), packet.get_internet_header_length()
                                    // );
                                    if packet.dst_is_subnet(&acoustic_domain, &acoustic_mask) {
                                        let _ = forward_acoustic_tx_copy.send(packet);
                                        continue;
                                    }
                                    let icmp_packet =
                                        ICMP::try_new_from_ip_packet(&packet).unwrap();
                                    let nat_table_handle = nat_table_clone.lock().unwrap();
                                    if nat_table_handle
                                        .contains_key(&(icmp_packet.get_utils() as u16))
                                    {
                                        println!("receive reply of acoustic packet");
                                        packet.set_destination_address(
                                            *nat_table_handle
                                                .get(&(icmp_packet.get_utils() as u16))
                                                .unwrap(),
                                        );
                                        packet.update_header_checksum();
                                        // println!("icmp data: {:?}", icmp_packet.get_payload());
                                        let _ = forward_acoustic_tx_copy.send(packet);
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        });
    }
    println!("Nat Forward Daemon start");
}

fn ip_packet_to_icmp<'a>(
    ip_packet: &IpPacket,
    icmp_header: &'a mut [u8],
) -> MutableEchoRequestPacket<'a> {
    let mut icmp = MutableEchoRequestPacket::new(icmp_header).unwrap();
    let local_icmp = ICMP::try_new_from_ip_packet(ip_packet).unwrap();
    icmp.set_icmp_type(match local_icmp.get_type() {
        ICMPType::EchoRequest => IcmpTypes::EchoRequest,
        ICMPType::EchoReply => IcmpTypes::EchoReply,
        ICMPType::Unsupported => IcmpTypes::DestinationUnreachable,
    });
    icmp.set_icmp_code(IcmpCodes::NoCode);
    icmp.set_sequence_number(local_icmp.get_sequence_number().unwrap());
    icmp.set_identifier(local_icmp.get_utils() as u16);
    icmp.set_payload(&local_icmp.get_payload());
    let checksum = checksum(icmp.packet(), 1);
    icmp.set_checksum(checksum);
    icmp
}

#[test]
fn test_pnet() {
    let interface_names_match = |iface: &NetworkInterface| iface.index == 7;

    // Find the network interface with the provided name
    let interfaces = datalink::interfaces();
    for i in &interfaces {
        println!("{:?}", i);
    }
    let interface = interfaces
        .into_iter()
        .filter(interface_names_match)
        .next()
        .unwrap();

    println!("selected: {:?}", interface);

    let protocol = Layer4(TransportProtocol::Ipv4(IpNextHeaderProtocols::Icmp));
    let (mut tx, mut rx) = transport_channel(4096, protocol).unwrap();
    let mut iter = icmp_packet_iter(&mut rx);

    loop {
        if let Ok(data) = iter.next() {
            println!("{:?}", data);
        }
    }
}
