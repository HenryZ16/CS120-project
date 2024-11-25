// Internet Control Message Protocol (ICMP)
// We only implement Ping
use crate::acoustic_ip::ip_packet::{
    IpPacket, IpProtocol, TestAdapter, SELF_ADDRESS, SELF_GATEWAY, SELF_MASK,
};
use std::net::Ipv4Addr;

#[derive(Debug, PartialEq, Eq)]
pub enum ICMPType {
    Unsupported = 255,
    EchoReply = 0,
    EchoRequest = 8,
}

#[derive(Clone)]
pub struct ICMP {
    icmp_type: u8,
    icmp_code: u8,
    checksum: u16,
    utils: u32,
    payload: Vec<u8>,
}

impl ICMP {
    pub fn try_new_from_bytes(bytes: &Vec<u8>) -> Result<ICMP, &'static str> {
        if bytes.len() < 8 {
            Err("ICMP packet too short")?;
        }
        let icmp_type = bytes[0];
        let icmp_code = bytes[1];
        let checksum = ((bytes[2] as u16) << 8) | (bytes[3] as u16);
        let utils = ((bytes[4] as u32) << 24)
            | ((bytes[5] as u32) << 16)
            | ((bytes[6] as u32) << 8)
            | (bytes[7] as u32);
        let payload = bytes[8..].to_vec();
        Ok(ICMP {
            icmp_type,
            icmp_code,
            checksum,
            utils,
            payload,
        })
    }
    pub fn try_new_from_ip_packet(packet: &IpPacket) -> Result<ICMP, &'static str> {
        if packet.get_protocol() != IpProtocol::ICMP {
            Err("Not an ICMP packet")?;
        }
        ICMP::try_new_from_bytes(&packet.get_data())
    }

    pub fn get_icmp_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.push(self.icmp_type);
        bytes.push(self.icmp_code);
        bytes.push((self.checksum >> 8) as u8);
        bytes.push((self.checksum & 0xff) as u8);
        bytes.push((self.utils >> 24) as u8);
        bytes.push((self.utils >> 16) as u8);
        bytes.push((self.utils >> 8) as u8);
        bytes.push((self.utils & 0xff) as u8);
        bytes.extend(self.payload.iter());
        bytes
    }

    pub fn calc_checksum(&self) -> u16 {
        let mut icmp_bytes = self.get_icmp_bytes();
        icmp_bytes[2] = 0;
        icmp_bytes[3] = 0;
        let mut sum = 0u32;
        let mut i = 0;
        while i < icmp_bytes.len() {
            let mut word = (icmp_bytes[i] as u16) << 8;
            if i + 1 < icmp_bytes.len() {
                word |= icmp_bytes[i + 1] as u16;
            }
            sum += word as u32;
            i += 2;
        }
        while sum >> 16 != 0 {
            sum = (sum & 0xffff) + (sum >> 16);
        }
        !sum as u16
    }
    pub fn check_checksum(&self) -> bool {
        self.calc_checksum() == self.checksum
    }

    // Only support Ping
    pub fn get_type(&self) -> ICMPType {
        match self.icmp_type {
            0 => ICMPType::EchoReply,
            8 => ICMPType::EchoRequest,
            _ => ICMPType::Unsupported,
        }
    }

    pub fn reply_echo(&self) -> ICMP {
        let mut reply = self.clone();
        reply.icmp_type = 0;
        reply.checksum = 0;
        reply.checksum = reply.calc_checksum();
        reply
    }
}

#[test]
fn test_local_ping() {
    let adapter = TestAdapter::new(SELF_ADDRESS, SELF_MASK, None);
    let mut grab_cnt = 4;
    while grab_cnt > 0 {
        let packet = match adapter.try_receive_blocking() {
            Ok(packet) => packet,
            Err(_) => continue,
        };
        let packet_bytes = packet.get_ip_packet_bytes();
        let ip_packet = IpPacket::new_from_bytes(&packet_bytes);
        if ip_packet.get_protocol() != IpProtocol::ICMP {
            continue;
        }

        if !ip_packet.check_header_checksum()
            || ip_packet.get_destination_address() != SELF_ADDRESS.to_bits()
        {
            continue;
        }

        let icmp = ICMP::try_new_from_ip_packet(&ip_packet).unwrap();
        if icmp.check_checksum() && icmp.get_type() == ICMPType::EchoRequest {
            println!("Received ICMP Echo Request, grab_cnt = {}", grab_cnt);

            let reply = icmp.reply_echo();
            let mut reply_packet = ip_packet.clone();
            reply_packet.set_destination_address(ip_packet.get_source_address());
            reply_packet.set_source_address(ip_packet.get_destination_address());
            reply_packet.set_data(reply.get_icmp_bytes());

            adapter.send(reply_packet);
        } else {
            continue;
        }

        grab_cnt -= 1;
    }
}
