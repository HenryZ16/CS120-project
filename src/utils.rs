use rand::Rng;
use std::fs::File;
use std::io::Write;

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

pub fn read_data_2_compressed_u8(data: Vec<u8>) -> Vec<u8> {
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

pub fn read_compressed_u8_2_data(data: Vec<u8>) -> Vec<u8> {
    let mut decompressed_data = vec![];
    for i in 0..data.len() {
        for j in 0..8 {
            decompressed_data.push((data[i] >> (7 - j)) & 1);
        }
    }
    return decompressed_data;
}
