use anyhow::{Error, Result};
use plotters::data;
use reed_solomon_erasure::galois_8::ReedSolomon;

pub const MAX_FRAME_DATA_LENGTH: usize = 480;
pub const FRAME_PAYLOAD_LENGTH: usize = 512;
pub const FRAME_LENGTH_LENGTH_REDUNDANCY: usize = 3;
pub const FRAME_PREAMBLE: u32 = 0b0101010101;
pub const FRAME_PREAMBLE_LENGTH: usize = 0;

const U8_MASK: u8 = 0b11111111;

pub fn frame_length_length() -> usize {
    FRAME_LENGTH_LENGTH_REDUNDANCY * 8
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
        preamble_length -= 8;
        while preamble_length > 0 {
            let byte = (preamble >> preamble_length) as u8;
            whole_frame_bits.push(byte);
            preamble_length -= 8;
        }

        println!("[get_whole_frame_bits] preamble: {:?}", whole_frame_bits);

        // Length
        // compute the length of length bits
        let mut length: u64 = 0;
        let length_length = (frame_length_length() / FRAME_LENGTH_LENGTH_REDUNDANCY) as isize;
        println!("[get_whole_frame_bits] length_length: {:?}", length_length);
        for i in (0..length_length).rev() {
            for _ in 0..FRAME_LENGTH_LENGTH_REDUNDANCY {
                length |= (self.length >> i) as u64 & 1;
                length <<= 1;
            }
        }
        length >>= 1;
        println!("[get_whole_frame_bits] self.length: {:?}", self.length);

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

        println!("[get_whole_frame_bits] length: {:?}", whole_frame_bits);

        // Payload
        let payload_length = (FRAME_PAYLOAD_LENGTH / 32) as isize;
        let payload = self.payload.clone();
        let mut loop_cnt = 0;
        for _ in 0..payload_length {
            for i in 0..4 {
                whole_frame_bits.push(payload[loop_cnt][i]);
            }
            loop_cnt += 1;
        }

        println!("[get_whole_frame_bits] payload: {:?}", whole_frame_bits);
        println!(
            "[get_whole_frame_bits] whole_frame_bits.len(): {:?}",
            whole_frame_bits.len()
        );

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
        let mut data_len = data.len();
        while data_len < FRAME_PAYLOAD_LENGTH / 8 {
            data.push(0);
            data_len += 1;
        }

        // construct the payload (to fit in the shard macro)
        let mut i = 0;
        let mut payload: Vec<Vec<u8>> = vec![];
        while i < FRAME_PAYLOAD_LENGTH / 8 {
            let mut payload_shard = vec![];
            for j in 0..4 {
                payload_shard.push(data[i + j]);
            }
            payload.push(payload_shard);
            i += 4;
        }

        // RS encoding
        let rs = ReedSolomon::new(
            MAX_FRAME_DATA_LENGTH / 32,
            (FRAME_PAYLOAD_LENGTH - MAX_FRAME_DATA_LENGTH) / 32,
        )
        .unwrap();
        rs.encode(&mut payload).unwrap();

        println!("payload: {:?}", payload);
        println!("payload length: {:?}", payload.len());

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

    pub fn construct_payload_format(input: Vec<u8>) -> Vec<Vec<u8>> {
        let mut payload = Vec::new();
        let mut i = 0;
        while i < input.len() {
            let mut payload_shard = Vec::new();
            for j in 0..4 {
                payload_shard.push(input[i + j]);
            }
            payload.push(payload_shard);
            i += 4;
        }

        payload
    }
}

pub struct SimpleFrame {
    data: Vec<u8>,
    sample_rate: u32,
    ref_signal: Vec<f32>,
}

use rand::Rng;

impl SimpleFrame {
    pub fn new(carrier_freq: u32, data_len: usize) -> Self {
        let ref_signal: Vec<f32> = (0..48000 / carrier_freq)
            .map(|x| (2.0 * std::f32::consts::PI * x as f32 / 48000.0 * carrier_freq as f32).sin())
            .collect();
        let mut data = vec![];
        let mut rng = rand::thread_rng();
        for _ in 0..data_len {
            data.push(rng.gen_bool(0.5) as u8);
        }

        use std::fs::File;
        use std::io::Write;
        println!("org data: {:?}", data);
        let mut writer = File::create("ref_signal.txt").unwrap();

        for &num in &data {
            let ch = (num as u8 + b'0');
            writer.write_all(&[ch]).unwrap();
        }

        SimpleFrame {
            data,
            sample_rate: 48000,
            ref_signal,
        }
    }

    pub fn into_audio(&self, redundent_times: usize) -> Vec<f32> {
        let mut redundent = 1;
        if redundent_times > 1 {
            redundent = redundent_times;
        }
        let mut res = gen_preamble(self.sample_rate);

        for &bit in &self.data {
            if bit == 0 {
                // res.extend(self.ref_signal.clone().into_iter());
                for _ in 0..redundent {
                    res.extend(self.ref_signal.clone().into_iter());
                }
            } else {
                for _ in 0..redundent {
                    res.extend(self.ref_signal.clone().into_iter().map(|x| -x));
                }
            }
        }

        res
    }
}

pub fn gen_preamble(sample_rate: u32) -> Vec<f32> {
    let start = 1e2;
    let end = 1e4;
    let half_length = 440;
    let dx: f64 = 1.0 / sample_rate as f64;
    let step = (end - start) as f64 / half_length as f64;
    let mut fp: Vec<f64> = (0..half_length).map(|i| start + i as f64 * step).collect();
    let fp_rev = fp.clone().into_iter().rev();
    fp.pop();
    fp.extend(fp_rev);

    let mut res = vec![];

    res.push(0.0);
    for i in 1..fp.len() {
        let trap_area = (fp[i] + fp[i - 1]) * dx / 2.0;
        res.push(res[i - 1] + trap_area);
    }

    res.into_iter()
        .map(|x| (2.0 * std::f64::consts::PI * x).sin() as f32)
        .collect()
}
