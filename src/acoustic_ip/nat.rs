use pcap::Device;
use pnet::datalink::Channel::Ethernet;
use pnet::datalink::{self, NetworkInterface};
use pnet::packet::ethernet::{EthernetPacket, MutableEthernetPacket};
use pnet::packet::icmp::IcmpType;
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
use std::thread::{self, panicking};
use std::{
    env,
    net::IpAddr,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};
use tokio::sync::mpsc::UnboundedReceiver;

use super::adapter::Adapter;
use super::ip_packet::{IpPacket, IpProtocol};
use super::protocols::icmp::{ICMPType, ICMP};

const ICMP_HEADER_SIZE: usize = 64;

// pub fn nat_listen_daemon(if_index: ) {
//     let protocol = Layer4(TransportProtocol::Ipv4(IpNextHeaderProtocols::Icmp));
//     let (mut tx, mut rx) = transport_channel(4096, protocol).unwrap();

//     let mut iter = icmp_packet_iter(&mut rx);
//     while let Ok(data) = iter.next() {

//     }
// }

pub fn nat_forward_daemon(mut forward_acoustic_rx: UnboundedReceiver<IpPacket>) {
    let protocol = Layer4(TransportProtocol::Ipv4(IpNextHeaderProtocols::Icmp));
    let (mut tx, mut rx) = transport_channel(128, protocol).unwrap();
    println!("Nat Forward Daemon start");
    thread::spawn(move || {
        while let Some(packet) = forward_acoustic_rx.blocking_recv() {
            // println!("received packets");
            match packet.get_protocol() {
                IpProtocol::ICMP => {
                    let mut icmp_header: [u8; ICMP_HEADER_SIZE] = [0; ICMP_HEADER_SIZE];
                    let new_packet = ip_packet_to_icmp(&packet, &mut icmp_header);
                    // println!("icmp dst: {:?}", packet.get_destination_ipv4_addr());
                    let _ = tx.send_to(new_packet, packet.get_destination_ipv4_addr().into());
                }
                _ => {}
            }
        }
    });

    // let protocol = Layer3()
    thread::spawn(move || {
        let mut iter = icmp_packet_iter(&mut rx);
        println!("start check packet");
        while let Ok(_) = iter.next() {
            println!("received ICMP");
        }
    });
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
    icmp.set_identifier(local_icmp.get_identifier() as u16);
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
