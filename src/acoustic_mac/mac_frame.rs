use crate::acoustic_modem::phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING;
use crate::utils::{Bit, Byte};
use std::vec;
pub type MacAddress = Byte;
pub struct MACFrame {
    dest: MacAddress,
    src: MacAddress,
    mac_type: Byte,
    payload: Vec<Byte>,
}
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum MACType {
    Data,
    Ack,
}

pub fn mactype_2_u8(mac_type: MACType) -> Byte {
    match mac_type {
        MACType::Data => 0,
        MACType::Ack => 1,
    }
}

pub fn u8_2_mactype(byte: Byte) -> MACType {
    match byte {
        0 => MACType::Data,
        1 => MACType::Ack,
        _ => panic!("Invalid MACType"),
    }
}

impl MACFrame {
    pub fn new(dest: Byte, src: Byte, mac_type: MACType, payload: Vec<Byte>) -> Self {
        assert!(payload.len() <= MAX_FRAME_DATA_LENGTH_NO_ENCODING);
        MACFrame {
            dest,
            src,
            mac_type: mactype_2_u8(mac_type),
            payload,
        }
    }

    pub fn get_whole_frame_bits(&self) -> Vec<Byte> {
        let mut res = vec![];

        res.push(self.dest);
        res.push(self.src);
        res.push(self.mac_type);
        res.extend(self.payload.clone());

        return res;
    }

    pub fn get_dst(data: &[Byte]) -> MacAddress {
        data[0]
    }

    pub fn get_src(data: &[Byte]) -> MacAddress {
        data[1]
    }

    pub fn get_type(data: &[Byte]) -> MACType {
        u8_2_mactype(data[2])
    }
}
