use std::vec;

use crate::utils::{self, Bit, Byte};
use anyhow::{Error, Result};
use code_rs::bits::Hexbit;
use code_rs::coding::reed_solomon;

pub const MAX_FRAME_DATA_LENGTH: usize = 128;
pub const FRAME_PAYLOAD_LENGTH: usize = 144;
pub const FRAME_LENGTH_LENGTH: usize = 12;
pub const FRAME_LENGTH_LENGTH_NO_ENCODING: usize = 16;

pub struct PHYFrame {
    length: usize,
    payload: Vec<Hexbit>,
}

impl PHYFrame {
    // Preamble: 01010101
    // Length: <2 Hexbits>
    // Data: <12 Hexbits>
    // Preserved for future use: <2 Hexbits>
    // Parity: <8 Hexbits>
    // Payload Total: <24 Hexbits>
    pub fn new(length: usize, data: Vec<Byte>) -> Self {
        let payload = PHYFrame::data_2_payload(data, length).unwrap();
        PHYFrame { length, payload }
    }

    pub fn new_no_encoding(length: usize, data: Vec<Byte>) -> (usize, Vec<Byte>) {
        let mut payload: Vec<Byte> = Vec::new();
        println!("length: {}", length);
        payload.push((length >> 8) as u8);
        payload.push((length & 0xff) as u8);
        payload.extend(data);
        while payload.len() < (MAX_FRAME_DATA_LENGTH + FRAME_LENGTH_LENGTH_NO_ENCODING) / 8 {
            payload.push(0);
        }

        (length, payload)
    }

    pub fn get_whole_frame_bits(&self) -> Vec<Bit> {
        // No PSK preamble. Just tranverse Vec<Hexbit> into Vec<u8>
        return utils::code_rs_hexbit_2_u8(self.payload.clone());
    }

    // the length of data must be less than or equal to MAX_FRAME_DATA_LENGTH bits.
    // length and data are encoded into payload
    pub fn data_2_payload(data: Vec<u8>, len: usize) -> Result<Vec<Hexbit>, Error> {
        if len > MAX_FRAME_DATA_LENGTH || data.len() * 8 > MAX_FRAME_DATA_LENGTH {
            let err_msg = format!(
                "Data length exceeds maximum frame data length: {}",
                MAX_FRAME_DATA_LENGTH
            );
            return Err(Error::msg(err_msg));
        }

        // add length info into data
        let mut hexbits_data = usize_length_2_hexbits_length(len);

        // extend the length of `data: Vec<u8>` to 96 bits, tranverse into `Vec<Hexbit>`
        let mut data = data;
        let mut data_len = data.len();
        while data_len < MAX_FRAME_DATA_LENGTH / 8 {
            data.push(0);
            data_len += 1;
        }
        hexbits_data.extend(utils::u8_2_code_rs_hexbit(data));

        // extend the length of `hexbits_data: Vec<Hexbit>` to 216 bits
        let mut hexbits_data_len = hexbits_data.len();
        while hexbits_data_len < FRAME_PAYLOAD_LENGTH / 6 {
            hexbits_data.push(Hexbit::new(0));
            hexbits_data_len += 1;
        }
        let data = hexbits_data;

        // RS encoding
        let mut array_data: [Hexbit; 24] = data.try_into().unwrap();
        reed_solomon::medium::encode(&mut array_data);
        let payload = array_data.to_vec();

        println!(
            "[data_2_payload] payload: {:?}, length: {}",
            payload,
            payload.len()
        );

        return Ok(payload);
    }

    // reconstruct & get back the data
    pub fn payload_2_data(payload: Vec<Hexbit>) -> Result<(Vec<Byte>, usize), Error> {
        // RS decoding
        let mut array_payload: [Hexbit; 24] = payload.try_into().unwrap();
        reed_solomon::medium::decode(&mut array_payload);
        let payload = array_payload.to_vec();

        // println!(
        //     "[payload_2_data] payload: {:?}, length: {}",
        //     payload,
        //     payload.len()
        // );

        // get the length
        let length = hexbits_length_2_usize_length(payload[0..2].to_vec());

        // get the data
        let data = utils::code_rs_hexbit_2_u8(payload[2..22].to_vec());

        return Ok((data, length));
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

    pub fn into_audio(&self, redundent_times: usize, padding_len: usize) -> Vec<f32> {
        let mut redundent = 1;
        if redundent_times > 1 {
            redundent = redundent_times;
        }
        let mut res: Vec<f32> = (0..1000).map(|x| (2.0 * std::f32::consts::PI * x as f32 / 48000.0 * 10000 as f32).sin()).collect();
        res.extend(gen_preamble(self.sample_rate).iter());
        let padding = vec![0.0; padding_len];
        res.extend(padding.iter());

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
    let start = 8e2;
    let end = 2000.0;
    let half_length = 280;
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

pub fn usize_length_2_hexbits_length(length: usize) -> Vec<Hexbit> {
    // FRAME_LEGNTH_LENGTH must be 12
    assert_eq!(FRAME_LENGTH_LENGTH, 12);

    // length must less than 2^12
    assert!(length < 4096);

    let hexbits_length = vec![
        Hexbit::new((length >> 6) as u8),
        Hexbit::new((length & 0b111111) as u8),
    ];
    return hexbits_length;
}

pub fn hexbits_length_2_usize_length(length: Vec<Hexbit>) -> usize {
    // FRAME_LEGNTH_LENGTH must be 12
    assert_eq!(FRAME_LENGTH_LENGTH, 12);

    // length.len must be 2
    assert_eq!(length.len(), 2);

    let mut len = 0;
    len |= length[0].bits() as usize;
    len <<= 6;
    len |= length[1].bits() as usize;

    return len;
}
