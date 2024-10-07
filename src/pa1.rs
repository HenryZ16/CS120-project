use crate::acoustic_modem::demodulation::Demodulation2;
use crate::acoustic_modem::modulation;
use crate::acoustic_modem::modulation::Modulator;
use crate::acoustic_modem::phy_frame;
use crate::pa0;
use crate::utils;
use anyhow::{Error, Result};
use tokio::time::{self, Duration};
// use cpal::{
//     traits::{DeviceTrait, HostTrait},
//     Device, Host, HostId, SampleRate, SupportedStreamConfig,
// };
use std::fs::File;
use std::io::Read;
use std::vec;

const CARRIER_LOW: u32 = 3000;
const CARRIER_INTERVAL: u32 = 1000;
const CARRIER_CNT: u32 = 4;
const SAMPLE_RATE: u32 = 48000;

pub async fn obj_2() -> Result<u32> {
    let mut modulator_1 = Modulator::new(vec![1000, 10000], 48000, false);
    modulator_1.test_carrier_wave().await;
    return Ok(0);
}

pub async fn obj_3_send() -> Result<u32> {
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
    let mut modulator = Modulator::new(
        vec![CARRIER_LOW, CARRIER_INTERVAL, CARRIER_CNT],
        sample_rate,
        true,
    );

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

pub async fn obj_3_send_file() -> Result<u32> {
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
    let mut modulator = Modulator::new(
        vec![CARRIER_LOW, CARRIER_INTERVAL, CARRIER_CNT],
        sample_rate,
        true,
    );

    // send
    modulator
        .send_bits_2_file(
            utils::read_data_2_compressed_u8(data.clone()),
            data.len() as isize,
            "testset/send.wav",
        )
        .await;

    println!(
        "[pa1-obj3-send] Total elapsed time: {:?}",
        t_start.elapsed()
    );

    return Ok(0);
}

pub async fn obj_3_recv_file() -> Result<u32> {
    let mut demodulator = Demodulation2::new(
        vec![CARRIER_LOW, CARRIER_INTERVAL, CARRIER_CNT],
        SAMPLE_RATE,
        "output.txt",
        modulation::REDUNDANT_PERIODS,
    );

    let mut decoded_data = vec![];
    let mut debug_vec = vec![];
    let handle = demodulator.listening(
        true,
        phy_frame::FRAME_PAYLOAD_LENGTH,
        &mut decoded_data,
        &mut debug_vec,
        vec![],
    );
    let handle = time::timeout(Duration::from_secs(15), handle);
    println!("[pa1-obj3-receive] Start");
    handle.await.unwrap_err();
    println!("[pa1-obj3-recrive] Stop");

    return Ok(0);
}

pub async fn pa1(sel: i32, additional_type: &str) -> Result<u32> {
    let available_sel = vec![0, 1, 2, 3];
    if !available_sel.contains(&sel) {
        return Err(Error::msg("Invalid selection"));
    }

    if sel == 0 || sel == 1 {
        println!("Objective 1 start");
        match pa0::pa0(0).await {
            Ok(_) => {}
            Err(e) => {
                println!("Error: {}", e);
            }
        }
        println!("Objective 1 end");
    }

    if sel == 0 || sel == 2 {
        println!("Objective 2 start");
        match obj_2().await {
            Ok(_) => {}
            Err(e) => {
                println!("Error: {}", e);
            }
        }
        println!("Objective 2 end");
    }

    if sel == 0 || sel == 3 {
        match additional_type {
            "send" => {
                println!("Objective 3 start");
                match obj_3_send().await {
                    Ok(_) => {}
                    Err(e) => {
                        println!("Error: {}", e);
                    }
                }
                println!("Objective 3 end");
            }
            "send_file" => {
                println!("Objective 3 start");
                match obj_3_send_file().await {
                    Ok(_) => {}
                    Err(e) => {
                        println!("Error: {}", e);
                    }
                }
                println!("Objective 3 end");
            }
            "receive_file" => {
                println!("Objective 3 start");
                match obj_3_recv_file().await {
                    Ok(_) => {
                        println!("Objective 3 stop successfully");
                    }
                    _ => {
                        println!("Objective 3 failed");
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
