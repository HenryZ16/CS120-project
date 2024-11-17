use crate::acoustic_mac::controller::MacController;
use crate::acoustic_mac::mac_frame::MacAddress;
use crate::acoustic_modem::generator::PhyLayerGenerator;
use crate::acoustic_modem::phy_frame;
use crate::asio_stream::read_wav_and_play;
use crate::utils::get_audio_device_and_config;
use anyhow::{Error, Result};
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::process;
use std::time::Instant;
use std::vec;
use tokio::time::sleep;
use tokio::time::timeout;
use tokio::time::{self, Duration};
const CONFIG_FILE: &str = "configuration/pa2.yml";

const RECEIVE_BYTE_NUM: usize = 6250;

const SENDER_ADDRESS: MacAddress = 1;
const RECEIVER_ADDRESS: MacAddress = 2;

const NODE_0_ADDRESS: MacAddress = 1;
const NODE_1_ADDRESS: MacAddress = 2;
const NODE_2_ADDRESS: MacAddress = 3;
const NODE_3_ADDRESS: MacAddress = 4;
const NONE_NODE: MacAddress = 0xF;

pub async fn obj_1_mac_send() -> Result<u32> {
    let address = SENDER_ADDRESS;
    let t_start = std::time::Instant::now();

    let dest: u8 = RECEIVER_ADDRESS;
    let mut sender = crate::acoustic_mac::send::MacSender::new(CONFIG_FILE, address);

    // read data from testset/data.bin
    let mut file = File::open("testset/data.bin")?;
    let mut data: Vec<u8> = vec![];
    file.read_to_end(&mut data)?;
    println!("[pa2-obj1-send] Elapsed time: {:?}", t_start.elapsed());

    // send
    let frames = sender.generate_digital_data_frames(data, dest);
    println!("[pa2-obj1-send] frames: {:?}", frames.len());
    for frame in &frames {
        sender.send_frame(frame).await;
    }
    //sender.send_frame(&frames[0]).await;

    println!(
        "[pa2-obj1-send] Total elapsed time: {:?}",
        t_start.elapsed()
    );

    return Ok(0);
}

pub async fn obj_1_send() -> Result<u32> {
    let t_start = std::time::Instant::now();

    // read data from testset/data.bin
    let mut file = File::open("testset/data.bin")?;
    let mut data: Vec<u8> = vec![];
    file.read_to_end(&mut data)?;

    let config = PhyLayerGenerator::new_from_yaml(CONFIG_FILE);
    let (cpal_device, cpal_config) =
        crate::utils::get_audio_device_and_config(config.get_sample_rate());
    let mut modulator = config.gen_modulator(cpal_device, cpal_config);
    println!("[pa2-obj1-send] Elapsed time: {:?}", t_start.elapsed());

    // send
    modulator
        .send_bits(data.clone(), data.len() as isize * 8)
        .await;

    println!(
        "[pa2-obj1-send] Total elapsed time: {:?}",
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
    let (cpal_device, cpal_config) =
        crate::utils::get_audio_device_and_config(config.get_sample_rate());
    let mut modulator = config.gen_modulator(cpal_device, cpal_config);

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
        "[pa2-obj1-send] Total elapsed time: {:?}",
        t_start.elapsed()
    );

    return Ok(0);
}

pub async fn obj_1_recv_file() -> Result<u32> {
    let ymal_config = PhyLayerGenerator::new_from_yaml(CONFIG_FILE);
    let (device, config) = get_audio_device_and_config(ymal_config.get_sample_rate());
    let mut demodulator = ymal_config.gen_demodulation(device, config);

    let mut decoded_data = vec![];
    let handle = demodulator.listening(&mut decoded_data);
    let handle = time::timeout(Duration::from_secs(5), handle);
    println!("[pa1-obj3-receive] Start");
    let _ = handle.await;
    let mut file = File::create("testset/output.txt").unwrap();
    // file.write_all(&decoded_data).unwrap();
    file.write_all(&mut decoded_data).unwrap();
    println!("[pa2-obj2-receive] Stop");

    return Ok(0);
}

pub async fn obj_2_send() -> Result<u32> {
    let t_start = std::time::Instant::now();

    // read data from testset/data.bin
    let mut file = File::open("testset/data.bin")?;
    let mut data: Vec<u8> = vec![];
    file.read_to_end(&mut data)?;

    let mut mac_controller = MacController::new(CONFIG_FILE, SENDER_ADDRESS);
    let mut tmp = vec![];
    let _ = mac_controller
        .task(&mut tmp, 0, data, RECEIVER_ADDRESS)
        .await;

    println!(
        "[pa2-obj2-send] Total elapsed time: {:?}",
        t_start.elapsed()
    );
    return Ok(0);
}

pub async fn obj_2_recv() -> Result<u32> {
    let t_start = Instant::now();

    let mut decoded_data = vec![];
    let mut mac_controller = MacController::new(CONFIG_FILE, RECEIVER_ADDRESS);
    let _ = mac_controller
        .task(&mut decoded_data, RECEIVE_BYTE_NUM, vec![], SENDER_ADDRESS)
        .await;

    // let _handle = timeout(Duration::from_secs(20), task_handle).await;
    let mut file = File::create("testset/output.txt").unwrap();
    // file.write_all(&decoded_data).unwrap();
    file.write_all(&mut decoded_data).unwrap();

    println!(
        "[pa2-obj2-receive] Total elapsed time: {:?}",
        t_start.elapsed()
    );
    Ok(0)
}

pub async fn obj_3(node_name: usize, to_send: usize) -> Result<u32> {
    let t_start = Instant::now();

    let mut decoded_data = vec![];
    let mut mac_controller = MacController::new(
        CONFIG_FILE,
        match node_name {
            0 => NODE_0_ADDRESS,
            1 => NODE_1_ADDRESS,
            2 => NODE_2_ADDRESS,
            3 => NODE_3_ADDRESS,
            _ => NONE_NODE,
        },
    );
    // read data from testset/data.bin
    let mut file = File::open("testset/data.bin")?;
    let mut data: Vec<u8> = vec![];
    file.read_to_end(&mut data)?;

    let _task_handle = mac_controller
        .task(
            &mut decoded_data,
            RECEIVE_BYTE_NUM,
            data,
            match node_name {
                0 => NODE_0_ADDRESS,
                1 => NODE_1_ADDRESS,
                2 => NODE_2_ADDRESS,
                3 => NODE_3_ADDRESS,
                _ => NONE_NODE,
            },
        )
        .await;

    let mut file = File::create("testset/output.txt").unwrap();
    // file.write_all(&decoded_data).unwrap();
    file.write_all(&mut decoded_data).unwrap();

    println!("[pa2-obj3] Total elapsed time: {:?}", t_start.elapsed());

    Ok(0)
}

pub async fn pa2(sel: i32, additional_type: &str) -> Result<u32> {
    let available_sel = vec![0, 1, 2, 3];
    if !available_sel.contains(&sel) {
        return Err(Error::msg("Invalid selection"));
    }

    if sel == 0 || sel == 1 {
        match additional_type {
            "mac_send" => {
                println!("Objective 1 start");
                match obj_1_mac_send().await {
                    Ok(_) => {}
                    Err(e) => {
                        println!("Error: {}", e);
                    }
                }
                println!("Objective 1 end");
            }
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

    if sel == 0 || sel == 2 {
        println!("Objective 2 start");
        match additional_type {
            "send" => match obj_2_send().await {
                Ok(_) => {}
                Err(e) => {
                    println!("Error: {}", e);
                }
            },
            "recv" => match obj_2_recv().await {
                Ok(_) => {}
                Err(e) => {
                    println!("Error: {}", e);
                }
            },
            _ => {
                println!("Unsupported function.");
            }
        }
        println!("Objective 2 end")
    }

    if sel == 0 || sel == 3 {
        println!("Objective 3 start");
        match additional_type {
            "node0" => match obj_3(0, 1).await {
                Ok(_) => {}
                Err(e) => {
                    println!("Error: {}", e);
                }
            },
            "node1" => match obj_3(1, 0).await {
                Ok(_) => {}
                Err(e) => {
                    println!("Error: {}", e);
                }
            },
            "node2" => match obj_3(2, 3).await {
                Ok(_) => {}
                Err(e) => {
                    println!("Error: {}", e);
                }
            },
            "node3" => match obj_3(3, 2).await {
                Ok(_) => {}
                Err(e) => {
                    println!("Error: {}", e);
                }
            },
            _ => {
                println!("Unsupported function.");
            }
        }
    }

    return Ok(0);
}

#[tokio::test]
async fn test_digital_communication() {
    use rand::Rng;
    let config = PhyLayerGenerator::new_from_yaml(CONFIG_FILE);
    let (cpal_device, cpal_config) =
        crate::utils::get_audio_device_and_config(config.get_sample_rate());
    let mut modulator = config.gen_modulator(cpal_device, cpal_config);
    let mut rng = rand::thread_rng();
    let sign: Vec<Vec<f32>> = vec![vec![-0.5, 0.5], vec![0.5, -0.5]];
    let mut data = vec![];
    let mut modulated_signal = phy_frame::gen_preamble(48000);
    modulated_signal.extend(vec![0.0; 30]);
    for _ in 0..100 {
        data.push(rng.gen_range(0..=1));
    }
    println!("data: {:?}", data);
    for i in 0..data.len() {
        modulated_signal.extend(sign[data[i]].clone());
    }

    modulator.send_modulated_signal(modulated_signal).await;
}
