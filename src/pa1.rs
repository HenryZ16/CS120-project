use crate::acoustic_modem::modulation::Modulator;
use crate::pa0;
use crate::utils;
use anyhow::{Error, Result};
// use cpal::{
//     traits::{DeviceTrait, HostTrait},
//     Device, Host, HostId, SampleRate, SupportedStreamConfig,
// };
use std::fs::File;
use std::io::Read;

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
    let carrier_freq = 6000;
    let mut modulator = Modulator::new(vec![carrier_freq], sample_rate, false);

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
    let carrier_freq = 6000;
    let mut modulator = Modulator::new(vec![carrier_freq], sample_rate, false);

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
            _ => {
                println!("Unsupported function.");
            }
        }
    }

    return Ok(0);
}
