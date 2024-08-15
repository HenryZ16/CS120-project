use anyhow::{Error, Result};
use num_traits::Float;
use reed_solomon_erasure::galois_8::ReedSolomon;

pub const MAX_FRAME_DATA_LENGTH: usize = 960;
pub const FRAME_PAYLOAD_LENGTH: usize = 1024;
pub const FRAME_LENGTH_LENGTH_REDUNDANCY: usize = 3;
pub const FRAME_PREAMBLE: u32 = 0b0101010101;
pub const FRAME_PREAMBLE_LENGTH: usize = 10;

const U8_MASK: u8 = 0b11111111;

pub fn frame_length_length() -> usize {
    FRAME_LENGTH_LENGTH_REDUNDANCY * (MAX_FRAME_DATA_LENGTH as f64).log2().ceil() as usize
}

pub struct PHYFrame {
    length: usize,
    payload: Vec<Vec<u8>>,
}

impl PHYFrame {
    // Preamble: 0101010101
    // Length: <10 bits>
    // Payload: <1024 bits>
    pub fn new(length: usize, data: Vec<u8>) -> Self {
        let payload = PHYFrame::data_2_payload(data, length).unwrap();
        PHYFrame { length, payload }
    }

    // vec![preamble 7:0], ...
    // vec![preamble 7:(8 - FRAME_PREAMBLE_LENGTH % 8) | length (7 - FRAME_PREAMBLE_LENGTH % 8):0],
    // vec![length 7:0], ...
    // vec![length 7:0], vec![payload 7:0],
    // vec![payload 7:0], ...
    pub fn get_whole_frame_bits(&self) -> Vec<u8> {
        // the length of length bits and preamble bits must be a multiple of 8
        assert_eq!((frame_length_length() + FRAME_PREAMBLE_LENGTH) % 8, 0);

        let mut whole_frame_bits: Vec<u8> = vec![];

        // Preamble
        let preamble = FRAME_PREAMBLE;
        let mut preamble_length = FRAME_PREAMBLE_LENGTH as isize;
        while preamble_length > 0 {
            preamble_length -= 8;
            let byte = (preamble >> preamble_length) as u8;
            whole_frame_bits.push(byte);
        }

        // Length
        let mut length: u64 = 0;
        let length_length = (frame_length_length() / FRAME_LENGTH_LENGTH_REDUNDANCY) as isize;
        for i in (length_length - 1)..-1 {
            for _ in 0..FRAME_LENGTH_LENGTH_REDUNDANCY {
                length |= (self.length >> i) as u64 & 1;
                length <<= 1;
            }
        }

        let mut length_length = frame_length_length() as isize;
        if preamble_length < 0 {
            length_length += preamble_length;
            let mut byte = (preamble << -preamble_length) as u8 & U8_MASK;
            byte |= (length >> length_length) as u8;
            whole_frame_bits.push(byte);
        }

        while length_length > 0 {
            length_length -= 8;
            let byte = (length >> length_length) as u8 & U8_MASK;
            whole_frame_bits.push(byte);
        }

        // Payload
        let mut payload_length = FRAME_PAYLOAD_LENGTH as isize;
        let mut payload = self.payload.clone();
        while payload_length > 0 {
            payload_length -= 8;
            let mut byte = 0;
            for i in 0..4 {
                byte |= (payload[0][i] as u32) << (3 - i) * 8;
            }
            whole_frame_bits.push(byte as u8);
            payload.remove(0);
        }

        return whole_frame_bits;
    }

    // the length of data must be less than or equal to 960 bits.
    pub fn data_2_payload(data: Vec<u8>, len: usize) -> Result<Vec<Vec<u8>>, Error> {
        if len > MAX_FRAME_DATA_LENGTH || data.len() * 8 > MAX_FRAME_DATA_LENGTH {
            let err_msg = format!(
                "Data length exceeds maximum frame data length: {}",
                MAX_FRAME_DATA_LENGTH
            );
            return Err(Error::msg(err_msg));
        }

        // extend the length of `data: Vec<u8>` to 1024 bits
        let mut data = data;
        let mut data_len = data.len() * 8;
        while data_len < MAX_FRAME_DATA_LENGTH {
            data.push(0);
            data_len += 8;
        }

        // construct the payload (to fit in the shard macro)
        let mut i = 0;
        let mut payload: Vec<Vec<u8>> = vec![];
        while i < FRAME_PAYLOAD_LENGTH {
            let mut payload_shard = vec![];
            for j in 0..4 {
                payload_shard.push(data[i + j]);
            }
            payload.push(payload_shard);
            i += 8;
        }

        // RS encoding
        let rs = ReedSolomon::new(
            MAX_FRAME_DATA_LENGTH / 32,
            (FRAME_PAYLOAD_LENGTH - MAX_FRAME_DATA_LENGTH) / 32,
        )
        .unwrap();
        rs.encode(&mut payload).unwrap();

        return Ok(payload);
    }

    // reconstruct & get back the data
    pub fn payload_2_data(payload: Vec<Vec<u8>>) -> Result<Vec<u8>, Error> {
        // RS reconstruction
        let rs = ReedSolomon::new(
            MAX_FRAME_DATA_LENGTH / 32,
            (FRAME_PAYLOAD_LENGTH - MAX_FRAME_DATA_LENGTH) / 32,
        )
        .unwrap();
        let mut shards: Vec<_> = payload.iter().cloned().map(Some).collect();
        rs.reconstruct(&mut shards).unwrap();

        // Convert back to normal shard arrangement
        let result: Vec<_> = shards.into_iter().filter_map(|x| x).collect();
        let mut data: Vec<u8> = vec![];
        for shard in result {
            for byte in shard {
                data.push(byte);
            }
        }

        return Ok(data);
    }
}
