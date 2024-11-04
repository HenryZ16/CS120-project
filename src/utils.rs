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
pub fn gen_random_bin_file(len: usize) {
    let data = gen_random_data(len * 8);
    let data = read_data_2_compressed_u8(data);
    let mut file = File::create("testset/data.bin").unwrap();
    file.write_all(&data).unwrap();
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
