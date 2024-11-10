use crate::acoustic_modem::generator::PhyLayerGenerator;
use crate::acoustic_modem::modulation::Modulator;
use crate::pa0;
use crate::utils;
use anyhow::{Error, Result};
use tokio::time::{self, Duration};
// use cpal::{
//     traits::{DeviceTrait, HostTrait},
//     Device, Host, HostId, SampleRate, SupportedStreamConfig,
// };
use crate::asio_stream::read_wav_and_play;
use std::fs::File;
use std::io::{Read, Write};
use std::vec;
const CARRIER: u32 = 4000;
const OFDM: bool = true;
const CONFIG_FILE: &str = "configuration/pa1.yml";

pub async fn obj_2() -> Result<u32> {
    let (device, config) = utils::get_audio_device_and_config(48000);
    let mut modulator_1 = Modulator::new(vec![1000, 10000], device, config, false);
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
    let (device, config) = utils::get_audio_device_and_config(sample_rate);
    let carrier_freq = CARRIER;
    let mut modulator = Modulator::new(vec![carrier_freq, carrier_freq * 2], device, config, OFDM);

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
    let (device, config) = utils::get_audio_device_and_config(sample_rate);
    let carrier_freq = CARRIER;
    let mut modulator = Modulator::new(vec![carrier_freq, carrier_freq * 2], device, config, OFDM);

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

pub async fn obj_3_recv_file() -> Result<u32> {
    // let data_len = if ENABLE_ECC {
    //     phy_frame::FRAME_PAYLOAD_LENGTH
    // } else {
    //     phy_frame::FRAME_LENGTH_LENGTH_NO_ENCODING
    //         + phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING
    //         + phy_frame::FRAME_CRC_LENGTH_NO_ENCODING
    // };
    // let bits_len = if ENABLE_ECC {
    //     phy_frame::MAX_FRAME_DATA_LENGTH
    // } else {
    //     phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING
    // };
    // let mut demodulator = Demodulation2::new(
    //     vec![CARRIER, CARRIER * 2],
    //     SAMPLE_RATE,
    //     modulation::REDUNDANT_PERIODS,
    //     OFDM,
    //     data_len,
    //     bits_len,
    // );

    let config = PhyLayerGenerator::new_from_yaml(CONFIG_FILE);
    let mut demodulator = config.gen_demodulation();

    let mut decoded_data = vec![];
    let handle = demodulator.listening(&mut decoded_data);
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
