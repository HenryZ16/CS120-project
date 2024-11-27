use crate::utils::Byte;
use std::net::Ipv4Addr;
use std::sync::Arc;
use wintun::{Adapter, Packet, Session};

#[derive(Debug, PartialEq, Eq)]
pub enum IpProtocol {
    ICMP = 1,
    IGMP = 2,
    TCP = 6,
    UDP = 17,
    ENCAP = 41,
    OSPF = 89,
    SCTP = 132,
    OTHER = -1,
}

#[derive(Debug, Clone)]
pub struct IpPacket {
    version: Byte,                // 4
    internet_header_length: Byte, // 4
    type_of_service: Byte,        // 8
    total_length: u16,            // 16
    identification: u16,          // 16
    flags: Byte,                  // 3
    fragment_offset: u16,         // 13
    time_to_live: Byte,           // 8
    protocol: Byte,               // 8
    header_checksum: u16,         // 16
    source_address: u32,          // 32
    destination_address: u32,     // 32
    // --- 160 bits or 20 bytes above ---
    options: Vec<Byte>,
    data: Vec<Byte>,
}

impl IpPacket {
    pub fn new_from_bytes(bytes: &Vec<Byte>) -> IpPacket {
        if bytes.len() < 20 {
            panic!("Invalid IP packet length");
        }
        if (bytes[0] >> 4) != 4 {
            panic!("Only IPv4 is supported");
        }

        let version = bytes[0] >> 4;
        let internet_header_length = bytes[0] & 0x0F;
        let type_of_service = bytes[1];
        let total_length = (bytes[2] as u16) << 8 | bytes[3] as u16;
        let identification = (bytes[4] as u16) << 8 | bytes[5] as u16;
        let flags = bytes[6] >> 5;
        let fragment_offset = ((bytes[6] & 0x1F) as u16) << 8 | bytes[7] as u16;
        let time_to_live = bytes[8];
        let protocol = bytes[9];
        let header_checksum = (bytes[10] as u16) << 8 | bytes[11] as u16;
        let source_address = (bytes[12] as u32) << 24
            | (bytes[13] as u32) << 16
            | (bytes[14] as u32) << 8
            | bytes[15] as u32;
        let destination_address = (bytes[16] as u32) << 24
            | (bytes[17] as u32) << 16
            | (bytes[18] as u32) << 8
            | bytes[19] as u32;
        // --- 160 bits or 20 bytes above ---
        let byte_length_options = (internet_header_length as usize - 5) * 4;
        let options = if byte_length_options > 0 {
            bytes[20..20 + byte_length_options].to_vec()
        } else {
            Vec::new()
        };
        let byte_length_data = bytes.len() - 20 - byte_length_options;
        let data = if byte_length_data > 0 {
            bytes[20 + byte_length_options..].to_vec()
        } else {
            Vec::new()
        };

        IpPacket {
            version,
            internet_header_length,
            type_of_service,
            total_length,
            identification,
            flags,
            fragment_offset,
            time_to_live,
            protocol,
            header_checksum,
            source_address,
            destination_address,
            options,
            data,
        }
    }

    pub fn get_ip_packet_bytes(&self) -> Vec<Byte> {
        let mut bytes = Vec::new();
        bytes.push((self.version << 4) | self.internet_header_length);
        bytes.push(self.type_of_service);
        bytes.push((self.total_length >> 8) as Byte);
        bytes.push(self.total_length as Byte);
        bytes.push((self.identification >> 8) as Byte);
        bytes.push(self.identification as Byte);
        bytes.push((self.flags << 5) | (self.fragment_offset >> 8) as Byte);
        bytes.push(self.fragment_offset as Byte);
        bytes.push(self.time_to_live);
        bytes.push(self.protocol);
        bytes.push((self.header_checksum >> 8) as Byte);
        bytes.push(self.header_checksum as Byte);
        bytes.push((self.source_address >> 24) as Byte);
        bytes.push((self.source_address >> 16) as Byte);
        bytes.push((self.source_address >> 8) as Byte);
        bytes.push(self.source_address as Byte);
        bytes.push((self.destination_address >> 24) as Byte);
        bytes.push((self.destination_address >> 16) as Byte);
        bytes.push((self.destination_address >> 8) as Byte);
        bytes.push(self.destination_address as Byte);
        bytes.extend(self.options.iter());
        bytes.extend(self.data.iter());
        bytes
    }

    pub fn get_version() -> Byte {
        4
    }

    // it has 4 bits that specify the number of 32-bit words in the header
    pub fn get_internet_header_length(&self) -> Byte {
        self.internet_header_length
    }

    // DSCP (6 bits) + ECN (2 bits)
    pub fn get_type_of_service(&self) -> Byte {
        self.type_of_service
    }

    pub fn get_total_length(&self) -> u16 {
        self.total_length
    }

    pub fn get_identification(&self) -> u16 {
        self.identification
    }

    // 3 bits of flags
    // [reserved][don't fragment][more fragments]
    pub fn if_dont_fragment(&self) -> bool {
        (self.flags & 0b010) != 0
    }
    pub fn if_more_fragments(&self) -> bool {
        (self.flags & 0b001) != 0
    }

    // 13 bits of fragment offset
    // measured in units of 8 octets (64 bits)
    pub fn get_fragment_offset(&self) -> u16 {
        self.fragment_offset
    }

    // TTL must be decremented by at least 1 when the packet is forwarded
    // decrement_time_to_live returns the TTL before decrementing
    // after decrementing, the crc must be recalculated
    // after TTL reaches 0, the packet must be discarded
    pub fn get_time_to_live(&self) -> Byte {
        self.time_to_live
    }
    pub fn decrement_time_to_live(&mut self) -> Option<Byte> {
        let old_ttl = self.time_to_live;
        self.time_to_live -= 1;
        self.header_checksum = self.calc_header_checksum();
        if old_ttl == 0 {
            None
        } else {
            Some(old_ttl)
        }
    }

    pub fn get_protocol(&self) -> IpProtocol {
        match self.protocol {
            1 => IpProtocol::ICMP,
            2 => IpProtocol::IGMP,
            6 => IpProtocol::TCP,
            17 => IpProtocol::UDP,
            41 => IpProtocol::ENCAP,
            89 => IpProtocol::OSPF,
            132 => IpProtocol::SCTP,
            _ => IpProtocol::OTHER,
        }
    }

    // checksum is calculated over the header only
    // it must be recomputed at each point where the header is modified
    pub fn calc_header_checksum(&self) -> u16 {
        let mut packet_bytes = self.get_ip_packet_bytes();
        packet_bytes[10] = 0;
        packet_bytes[11] = 0;
        let mut sum = 0u32;
        let mut i = 0;
        while i < self.get_internet_header_length() as usize * 4 {
            sum += (packet_bytes[i] as u32) << 8 | packet_bytes[i + 1] as u32;
            i += 2;
        }
        while sum >> 16 != 0 {
            sum = (sum & 0xFFFF) + (sum >> 16);
        }
        !sum as u16
    }
    pub fn check_header_checksum(&self) -> bool {
        let packet_bytes = self.get_ip_packet_bytes();
        self.calc_header_checksum() == (packet_bytes[10] as u16) << 8 | packet_bytes[11] as u16
    }

    pub fn get_source_address(&self) -> u32 {
        self.source_address
    }
    pub fn set_source_address(&mut self, source_address: u32) {
        self.source_address = source_address;
    }

    pub fn get_destination_address(&self) -> u32 {
        self.destination_address
    }
    pub fn get_destination_ipv4_addr(&self) -> Ipv4Addr {
        Ipv4Addr::from(self.get_destination_address())
    }
    pub fn set_destination_address(&mut self, destination_address: u32) {
        self.destination_address = destination_address;
    }

    pub fn get_options(&self) -> Vec<Byte> {
        self.options.clone()
    }

    pub fn get_data(&self) -> Vec<Byte> {
        self.data.clone()
    }
    pub fn set_data(&mut self, data: Vec<Byte>) {
        self.data = data;
    }

    pub fn dst_is_subnet(&self, domain: &Ipv4Addr, mask: &Ipv4Addr) -> bool {
        (domain & mask) == (Ipv4Addr::from(self.get_destination_address()) & mask)
    }
}

pub struct TestAdapter {
    ip_addr: Ipv4Addr,
    ip_mask: Ipv4Addr,
    ip_gateway: Option<Ipv4Addr>,
    session: Arc<Session>,
}

impl TestAdapter {
    pub fn new(ip_addr: Ipv4Addr, ip_mask: Ipv4Addr, ip_gateway: Option<Ipv4Addr>) -> TestAdapter {
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
        TestAdapter {
            ip_addr,
            ip_mask,
            ip_gateway,
            session,
        }
    }

    pub fn try_receive_blocking(&self) -> Result<IpPacket, &'static str> {
        let packet = self.session.receive_blocking().unwrap();
        let packet_bytes = packet.bytes().to_vec();
        if packet_bytes[0] >> 4 != 4 {
            Err("Only IPv4 is supported")?;
        }
        Ok(IpPacket::new_from_bytes(&packet_bytes))
    }

    pub fn send(&self, packet: IpPacket) {
        let packet_bytes = packet.get_ip_packet_bytes();
        let mut return_packet = self
            .session
            .allocate_send_packet(packet_bytes.len() as u16)
            .unwrap();
        let return_bytes = return_packet.bytes_mut();
        return_bytes.copy_from_slice(&packet_bytes);
        self.session.send_packet(return_packet);
    }
}

pub const SELF_ADDRESS: Ipv4Addr = Ipv4Addr::new(172, 18, 3, 3);
pub const SELF_MASK: Ipv4Addr = Ipv4Addr::new(255, 255, 0, 0);
pub const SELF_GATEWAY: Ipv4Addr = Ipv4Addr::new(172, 18, 0, 1);

#[test]
fn test_get_protocol() {
    let adapter = TestAdapter::new(SELF_ADDRESS, SELF_MASK, Some(SELF_GATEWAY));
    let mut grab_cnt = 10;
    while grab_cnt > 0 {
        let packet = match adapter.try_receive_blocking() {
            Ok(packet) => packet,
            Err(_) => continue,
        };
        let packet_bytes = packet.get_ip_packet_bytes();
        let ip_packet = IpPacket::new_from_bytes(&packet_bytes);
        println!(
            "Length: {:?},Received packet: {:?}",
            packet_bytes.len(),
            packet_bytes
        );
        println!("Protocol: {:?}", ip_packet.get_protocol());
        grab_cnt -= 1;
    }
}

#[test]
fn test_checksum() {
    let adapter = TestAdapter::new(SELF_ADDRESS, SELF_MASK, Some(SELF_GATEWAY));
    let mut grab_cnt = 10;
    while grab_cnt > 0 {
        let packet = match adapter.try_receive_blocking() {
            Ok(packet) => packet,
            Err(_) => continue,
        };
        let packet_bytes = packet.get_ip_packet_bytes();
        let ip_packet = IpPacket::new_from_bytes(&packet_bytes);
        println!(
            "Length: {:?},Received packet: {:X?}",
            packet_bytes.len(),
            packet_bytes
        );
        println!(
            "Checksum: 0x{:X}, result: {:?}\n",
            ip_packet.calc_header_checksum(),
            ip_packet.check_header_checksum()
        );
        grab_cnt -= 1;
    }
}
