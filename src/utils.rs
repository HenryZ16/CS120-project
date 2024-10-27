use code_rs;
use rand::Rng;
use std::fs::File;
use std::io::Write;

pub type Bit = u8;
pub type Byte = u8;

pub fn gen_random_data(len: usize) -> Vec<u8> {
    let mut rng = rand::thread_rng();
    let mut data = vec![];
    for _ in 0..len {
        data.push(rng.gen_range(0..2));
    }
    return data;
}

pub fn gen_random_data_file(len: usize) {
    let data = gen_random_data(len);
    let mut file = File::create("testset/data.txt").unwrap();
    for i in 0..len {
        file.write_all(data[i].to_string().as_bytes()).unwrap();
    }
}

pub fn read_data_2_compressed_u8(data: Vec<Bit>) -> Vec<Byte> {
    let mut compressed_data = vec![];
    let mut cnt = 0;
    let mut byte = 0;
    for i in 0..data.len() {
        byte |= data[i] << (7 - cnt);
        cnt += 1;
        if cnt == 8 {
            compressed_data.push(byte);
            byte = 0;
            cnt = 0;
        }
    }
    if cnt != 0 {
        compressed_data.push(byte);
    }
    return compressed_data;
}

pub fn read_compressed_u8_2_data(data: Vec<Byte>) -> Vec<Bit> {
    let mut decompressed_data = vec![];
    for i in 0..data.len() {
        for j in 0..8 {
            decompressed_data.push((data[i] >> (7 - j)) & 1);
        }
    }
    return decompressed_data;
}

pub fn u8_2_code_rs_hexbit(data: Vec<Byte>) -> Vec<code_rs::bits::Hexbit> {
    use code_rs::bits::Hexbit;

    // ensure that the length of data bits is a common divisor of 6 and 8: length mod 3 = 0
    // assert_eq!(data.len() % 3, 0);

    let mut hexbits = vec![];
    for i in (0..data.len()).step_by(3) {
        // [7-2][1-0 + 7-4][3-0 + 7-6][5-0]
        hexbits.push(Hexbit::new((data[i] >> 2) as u8));
        hexbits.push(Hexbit::new(((data[i] & 0b11) << 4) | (data[i + 1] >> 4)));
        hexbits.push(Hexbit::new(
            ((data[i + 1] & 0b1111) << 2) | (data[i + 2] >> 6),
        ));
        hexbits.push(Hexbit::new(data[i + 2] & 0b111111));
    }

    return hexbits;
}

pub fn code_rs_hexbit_2_u8(data: Vec<code_rs::bits::Hexbit>) -> Vec<Byte> {
    // ensure that the length of data bits is a common divisor of 6 and 8: mod 4
    assert_eq!(data.len() % 4, 0);

    let mut u8s = vec![];
    for i in (0..data.len()).step_by(4) {
        // [5-0 + 5-4][3-0 + 5-2][1-0 + 5-0]
        u8s.push((data[i].bits() << 2) | (data[i + 1].bits() >> 4));
        u8s.push((data[i + 1].bits() << 4) | (data[i + 2].bits() >> 2));
        u8s.push((data[i + 2].bits() << 6) | data[i + 3].bits());
    }

    return u8s;
}

// encoder & decoder used for convolutional coding
pub fn code_rs_encode(data: &mut [code_rs::bits::Hexbit; 24]) {
    let conv_code: ConvCode = ConvCode::new();
    for i in (0..16).step_by(2) {
        let mut encode_data: u32 = ((data[i].bits() as u32) << 6) | (data[i + 1].bits() as u32);

        encode_data = conv_code.rs_encode_12_6(encode_data);

        data[i] = code_rs::bits::Hexbit::new(((encode_data >> 12) as u8) & 0b111111);
        data[i + 1] = code_rs::bits::Hexbit::new(((encode_data >> 6) as u8) & 0b111111);
        data[i / 2 + 16] = code_rs::bits::Hexbit::new((encode_data as u8) & 0b111111);
    }
}

pub fn code_rs_decode(data: &mut [code_rs::bits::Hexbit; 24]) {
    let conv_code: ConvCode = ConvCode::new();
    for i in (0..16).step_by(2) {
        let mut decode_data: u32 = ((data[i].bits() as u32) << 12)
            | ((data[i + 1].bits() as u32) << 6)
            | data[i / 2 + 16].bits() as u32;

        decode_data = conv_code.rs_decode_12_6(decode_data);

        data[i] = code_rs::bits::Hexbit::new(((decode_data >> 6) as u8) & 0b111111);
        data[i + 1] = code_rs::bits::Hexbit::new((decode_data as u8) & 0b111111);
    }
}

const TRELLIS_STATES: usize = 8;
const NUM_INPUT_BITS: usize = 2;
const NUM_OUTPUT_BITS: usize = 3;
pub struct ConvCode {
    states: [[(u8, u32); 4]; TRELLIS_STATES],
}
impl ConvCode {
    pub fn new() -> Self {
        let states = ConvCode::generate_states();
        ConvCode { states }
    }

    fn generate_states() -> [[(u8, u32); 4]; TRELLIS_STATES] {
        let mut states = [[(0, 0); 4]; TRELLIS_STATES];

        for state in 0..TRELLIS_STATES {
            for input in 0..(1 << NUM_INPUT_BITS) {
                let u1 = (input >> 1) & 1;
                let u2 = input & 1;
                let r1 = (state >> 2) & 1;
                let r2 = (state >> 1) & 1;
                let r3 = state & 1;
                let v1 = u1 ^ r1 ^ r2;
                let v2 = u2 ^ r1 ^ r3;
                let v3 = u1 ^ u2 ^ r2;

                let encoded_bits = ((v1 << 2) | (v2 << 1) | v3) as u8;
                states[state][input] = (
                    encoded_bits,
                    ((state >> 2 & 1) | (u1 << 2) | (u2 << 1)) as u32,
                );
            }
        }

        states
    }

    pub fn rs_encode_12_6(&self, data: u32) -> u32 {
        let mut encoded_data: u32 = 0;
        let mut state: u32 = 0;
        for i in (0..12).step_by(2) {
            let input = (data >> (10 - i)) & 0b11;
            let (encoded_bits, next_state) = self.states[state as usize][input as usize];
            encoded_data = (encoded_data << 3) | encoded_bits as u32;
            state = next_state;
        }
        return encoded_data;
    }

    fn rs_decode_12_6(&self, encoded_data: u32) -> u32 {
        let mut paths = vec![vec![0u8; 6]; TRELLIS_STATES];
        let mut path_metrics = [u32::MAX; TRELLIS_STATES];
        let states: [[(u8, u32); 4]; TRELLIS_STATES] = self.states;
        path_metrics[0] = 0;

        for i in (0..18).step_by(NUM_OUTPUT_BITS) {
            let received_bits = ((encoded_data >> (15 - i)) & 0b111) as u8;
            let mut new_path_metrics = [u32::MAX; TRELLIS_STATES];
            let mut new_paths = vec![vec![0u8; 6]; TRELLIS_STATES];
            for state in 0..TRELLIS_STATES {
                for input in 0..(1 << NUM_INPUT_BITS) {
                    let (encoded_bits, next_state) = states[state][input];
                    let next_state = next_state as usize;
                    let branch_metric = (encoded_bits ^ received_bits).count_ones();
                    let path_metric = path_metrics[state].saturating_add(branch_metric);
                    if path_metric < new_path_metrics[next_state] {
                        new_path_metrics[next_state] = path_metric;
                        new_paths[next_state] = paths[state].clone();
                        new_paths[next_state][i / NUM_OUTPUT_BITS] = input as u8;
                    }
                }
            }
            path_metrics.copy_from_slice(&new_path_metrics);
            for state in 0..TRELLIS_STATES {
                paths[state].copy_from_slice(&new_paths[state]);
            }
        }

        let min_state = path_metrics
            .iter()
            .enumerate()
            .min_by_key(|&(_, &metric)| metric)
            .map(|(state, _)| state)
            .unwrap();

        let mut decoded_data = 0;
        for &bit in &paths[min_state] {
            decoded_data = (decoded_data << NUM_INPUT_BITS) | (bit as u32);
        }

        decoded_data
    }
}

#[test]
pub fn test_cv_no_corruption() {
    let data = 0b110101011001;
    let conv_code = ConvCode::new();
    let encoded_data = conv_code.rs_encode_12_6(data);
    let decoded_data = conv_code.rs_decode_12_6(encoded_data);
    assert_eq!(data, decoded_data);
}

#[test]
pub fn test_cv_with_corruption() {
    let data = 0b110101011001;
    let err = 0b000010000000000000;
    let conv_code = ConvCode::new();
    let encoded_data = conv_code.rs_encode_12_6(data);
    let corrupted_data = encoded_data ^ err;
    let decoded_data = conv_code.rs_decode_12_6(corrupted_data);
    assert_eq!(data, decoded_data);
}

#[test]
pub fn test_cv_corruption_24_hexbits() {
    let mut data = [code_rs::bits::Hexbit::new(0); 24];
    for i in 0..16 {
        data[i] = code_rs::bits::Hexbit::new(i as u8);
    }
    code_rs_encode(&mut data);
    data[4] = code_rs::bits::Hexbit::new(data[4].bits() ^ 0b001001);
    code_rs_decode(&mut data);
    for i in 0..16 {
        assert_eq!(data[i].bits(), i as u8);
    }
}

#[test]
pub fn fail_test_cv_corruption_24_hexbits() {
    println!("Too many errors that make it impossible to correct");
    let mut data = [code_rs::bits::Hexbit::new(0); 24];
    for i in 0..16 {
        data[i] = code_rs::bits::Hexbit::new(i as u8);
    }
    code_rs_encode(&mut data);
    data[4] = code_rs::bits::Hexbit::new(data[4].bits() ^ 0b111001);
    code_rs_decode(&mut data);
    for i in 0..16 {
        assert_eq!(data[i].bits(), i as u8);
    }
}
