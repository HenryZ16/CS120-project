use crate::acoustic_modem::generator::PhyLayerGenerator;
use crate::asio_stream::read_wav_and_play;
use anyhow::{Error, Result};
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::vec;
use tokio::join;
use tokio::time::{self, Duration};

const CONFIG_FILE: &str = "configuration/pa2.yml";

pub async fn obj_1_send() -> Result<u32> {
    let t_start = std::time::Instant::now();

    // read data from testset/data.bin
    let mut file = File::open("testset/data.bin")?;
    let mut data: Vec<u8> = vec![];
    file.read_to_end(&mut data)?;

    let config = PhyLayerGenerator::new_from_yaml(CONFIG_FILE);
    let mut modulator = config.gen_modulator();

    // modulator
    // let sample_rate = 48000;
    // let carrier_freq = CARRIER;
    // let mut modulator = Modulator::new(vec![carrier_freq, carrier_freq * 2], sample_rate, OFDM);

    // send
    modulator
        .send_bits(data.clone(), data.len() as isize * 8)
        .await;

    println!(
        "[pa1-obj3-send] Total elapsed time: {:?}",
        t_start.elapsed()
    );

    return Ok(0);
}

pub async fn obj_1_send_file() -> Result<u32> {
    let t_start = std::time::Instant::now();

    // read data from testset/data.bin
    let mut file = File::open("testset/data.bin")?;
    let mut data: Vec<u8> = vec![];
    file.read_to_end(&mut data)?;

    // println!("data: {:?}", data);
    let config = PhyLayerGenerator::new_from_yaml(CONFIG_FILE);
    let mut modulator = config.gen_modulator();

    // modulator
    // let sample_rate = 48000;
    // let carrier_freq = CARRIER;
    // let mut modulator = Modulator::new(vec![carrier_freq, carrier_freq * 2], sample_rate, OFDM);

    let file = "testset/send.wav";
    // send
    modulator
        .send_bits_2_file(data.clone(), data.len() as isize * 8, &file)
        .await;

    read_wav_and_play(&file).await;

    println!(
        "[pa1-obj3-send] Total elapsed time: {:?}",
        t_start.elapsed()
    );

    return Ok(0);
}

pub async fn obj_1_recv_file() -> Result<u32> {
    let config = PhyLayerGenerator::new_from_yaml(CONFIG_FILE);
    let mut demodulator = config.gen_demodulation();

    let mut decoded_data = vec![];
    let handle = demodulator.listening(&mut decoded_data);
    let handle = time::timeout(Duration::from_secs(10), handle);
    println!("[pa1-obj3-receive] Start");
    handle.await;
    let mut file = File::create("testset/output.txt").unwrap();
    // file.write_all(&decoded_data).unwrap();
    file.write_all(&mut decoded_data).unwrap();
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
            "test" => {
                let handle_recv = obj_1_recv_file();
                let handle_send = obj_1_send_file();

                join!(handle_recv, handle_send);
            }
            _ => {
                println!("Unsupported function.");
            }
        }
    }

    return Ok(0);
}
