use plotters::data;

use crate::utils::{Bit, Byte};
use std::{io::Bytes, vec};

pub const DEST_MASK: Byte = 0b11110000;
pub const SRC_MASK: Byte = 0b00001111;
pub const ID_MASK: Byte = 0b11111100;
pub const TYPE_MASK: Byte = 0b00000011;

// Frame:: [dest][src][type][payload]
pub type MacAddress = Byte;
pub struct MACFrame {
    frame_id: Byte,
    dest: MacAddress,
    src: MacAddress,
    mac_type: Byte,
    payload: Vec<Byte>,
}
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum MACType {
    Data,
    Ack,
    Unknown,
}

pub fn mactype_2_u8(mac_type: MACType) -> Byte {
    match mac_type {
        MACType::Data => 0,
        MACType::Ack => 1,
        MACType::Unknown => 2,
    }
}

pub fn u8_2_mactype(byte: Byte) -> MACType {
    match byte {
        0 => MACType::Data,
        1 => MACType::Ack,
        _ => MACType::Unknown,
    }
}

impl MACFrame {
    pub fn new(dest: Byte, src: Byte, mac_type: MACType, payload: Vec<Byte>) -> Self {
        MACFrame {
            frame_id: 0,
            dest,
            src,
            mac_type: mactype_2_u8(mac_type),
            payload,
        }
    }

    // Frame:: [dest : 7-4][src : 3-0][id : 7-2][type : 1-0][payload]
    pub fn get_whole_frame_bits(&self) -> Vec<Byte> {
        let byte_0 = ((self.dest << 4) & DEST_MASK) | (self.src & SRC_MASK);
        let byte_1 = (self.frame_id << 2) & ID_MASK | (self.mac_type & TYPE_MASK);
        let mut res = vec![];

        res.push(byte_0);
        res.push(byte_1);
        res.extend(self.payload.clone());

        return res;
    }

    pub fn get_frame_id(data: &[Byte]) -> Byte {
        (data[1] & ID_MASK) >> 2
    }

    pub fn set_frame_id(&mut self, frame_id: Byte) {
        self.frame_id = frame_id;
    }

    pub fn get_dst(data: &[Byte]) -> MacAddress {
        (data[1] & DEST_MASK) >> 4
    }

    pub fn get_src(data: &[Byte]) -> MacAddress {
        data[2] & SRC_MASK
    }

    pub fn get_type(data: &[Byte]) -> MACType {
        u8_2_mactype(data[3] & TYPE_MASK)
    }

    pub fn get_payload(data: &[Byte]) -> &[Byte] {
        &data[2..data.len()]
    }
}
