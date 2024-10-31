use crate::acoustic_modem::demodulation::Demodulation2;
use crate::acoustic_modem::modulation;
use crate::acoustic_modem::modulation::Modulator;
use crate::acoustic_modem::modulation::ENABLE_ECC;
use crate::acoustic_modem::phy_frame;
use crate::asio_stream::read_wav_and_play;
use crate::utils;
use anyhow::{Error, Result};
use std::fs::File;
use std::io::{Read, Write};
use std::vec;
use tokio::time::{self, Duration};
const CARRIER: u32 = 6000;
const SAMPLE_RATE: u32 = 48000;
const OFDM: bool = true;

pub async fn obj_1_send() -> Result<u32> {
    let t_start = std::time::Instant::now();

    // read data from testset/data.txt
    let mut file = File::open("testset/data.txt")?;
    let mut data = String::new();
    file.read_to_string(&mut data)?;
    let data = data
        .chars()
        .map(|c| c.to_digit(10).unwrap() as u8)
        .collect::<Vec<u8>>();

    // modulator
    let sample_rate = 48000;
    let carrier_freq = CARRIER;
    let mut modulator = Modulator::new(vec![carrier_freq, carrier_freq * 2], sample_rate, OFDM);

    // send
    modulator
        .send_bits(
            utils::read_data_2_compressed_u8(data.clone()),
            data.len() as isize,
        )
        .await;

    println!(
        "[pa1-obj3-send] Total elapsed time: {:?}",
        t_start.elapsed()
    );

    return Ok(0);
}

pub async fn obj_1_send_file() -> Result<u32> {
    let t_start = std::time::Instant::now();

    // read data from testset/data.txt
    let mut file = File::open("testset/data.txt")?;
    let mut data = String::new();
    file.read_to_string(&mut data)?;
    let data = data
        .chars()
        .map(|c| c.to_digit(10).unwrap() as u8)
        .collect::<Vec<u8>>();

    // modulator
    let sample_rate = 48000;
    let carrier_freq = CARRIER;
    let mut modulator = Modulator::new(vec![carrier_freq, carrier_freq * 2], sample_rate, OFDM);

    let file = "testset/send.wav";
    // send
    modulator
        .send_bits_2_file(
            utils::read_data_2_compressed_u8(data.clone()),
            data.len() as isize,
            &file,
        )
        .await;

    read_wav_and_play(&file).await;

    println!(
        "[pa1-obj3-send] Total elapsed time: {:?}",
        t_start.elapsed()
    );

    return Ok(0);
}

pub async fn obj_1_recv_file() -> Result<u32> {
    let mut demodulator = Demodulation2::new(
        vec![CARRIER, CARRIER * 2],
        SAMPLE_RATE,
        "other.txt",
        modulation::REDUNDANT_PERIODS,
        OFDM,
    );

    let data_len = if ENABLE_ECC {
        phy_frame::FRAME_PAYLOAD_LENGTH
    } else {
        phy_frame::FRAME_LENGTH_LENGTH_NO_ENCODING
            + phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING
            + phy_frame::FRAME_CRC_LENGTH_NO_ENCODING
    };

    let mut decoded_data = vec![];
    let mut debug_vec = vec![];
    let handle = demodulator.listening(false, data_len, &mut decoded_data, &mut debug_vec);
    let handle = time::timeout(Duration::from_secs(10), handle);
    println!("[pa1-obj3-receive] Start");
    handle.await.unwrap_err();
    let mut file = File::create("testset/output.txt").unwrap();
    // file.write_all(&decoded_data).unwrap();
    file.write_all(&decoded_data.iter().map(|x| x + b'0').collect::<Vec<u8>>())
        .unwrap();
    println!("[pa1-obj3-recrive] Stop");

    return Ok(0);
}

pub async fn pa2(sel: i32, additional_type: &str) -> Result<u32> {
    let available_sel = vec![0, 1, 2, 3];
    if !available_sel.contains(&sel) {
        return Err(Error::msg("Invalid selection"));
    }

    if sel == 0 || sel == 1 {
        match additional_type {
            "send" => {
                println!("Objective 1 start");
                match obj_1_send().await {
                    Ok(_) => {}
                    Err(e) => {
                        println!("Error: {}", e);
                    }
                }
                println!("Objective 1 end");
            }
            "send_file" => {
                println!("Objective 1 start");
                match obj_1_send_file().await {
                    Ok(_) => {}
                    Err(e) => {
                        println!("Error: {}", e);
                    }
                }
                println!("Objective 1 end");
            }
            "receive_file" => {
                println!("Objective 1 start");
                match obj_1_recv_file().await {
                    Ok(_) => {
                        println!("Objective 1 stop successfully");
                    }
                    _ => {
                        println!("Objective 1 failed");
                    }
                }
            }
            _ => {
                println!("Unsupported function.");
            }
        }
    }

    return Ok(0);
}
