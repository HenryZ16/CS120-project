use crate::utils::Byte;
use std::vec;

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
        MACFrame {
            frame_id: 0,
            dest,
            src,
            mac_type: mactype_2_u8(mac_type),
            payload,
        }
    }

    pub fn get_whole_frame_bits(&self) -> Vec<Byte> {
        let mut res = vec![self.frame_id];

        res.push(self.dest);
        res.push(self.src);
        res.push(self.mac_type);
        res.extend(self.payload.clone());

        return res;
    }

    pub fn get_frame_id(data: &[Byte]) -> Byte {
        data[0]
    }

    pub fn set_frame_id(&mut self, frame_id: Byte) {
        self.frame_id = frame_id;
    }

    pub fn get_dst(data: &[Byte]) -> MacAddress {
        data[1]
    }

    pub fn get_src(data: &[Byte]) -> MacAddress {
        data[2]
    }

    pub fn get_type(data: &[Byte]) -> MACType {
        u8_2_mactype(data[3])
    }

    pub fn get_payload(data: &[Byte]) -> &[Byte] {
        &data[4..data.len()]
    }
}
