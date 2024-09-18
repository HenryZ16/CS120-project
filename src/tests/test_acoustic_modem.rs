use std::fs::File;
use std::io::Read;
use std::time::Duration;
use std::vec;

use crate::acoustic_modem;
use crate::acoustic_modem::demodulation::{Demodulation, Demodulation2};
use crate::acoustic_modem::modulation::Modulator;
use crate::utils;
use plotters::{data, prelude::*};
use rand::thread_rng;
use rand::Rng;
use rand_distr::Normal;
use tokio::task;
use tokio::time::sleep;


fn plot(modulated_signal: Vec<f32>) -> Result<(), Box<dyn std::error::Error>> {
    // get the first 10000 samples
    let mut coordinates = vec![];
    for (i, sample) in modulated_signal.iter().enumerate() {
        if i > 10000 {
            break;
        }
        coordinates.push((i as f64, *sample as f64));
    }

    let drawing_area =
        SVGBackend::new("testset/modulated_data_wave.svg", (30000, 200)).into_drawing_area();
    drawing_area.fill(&WHITE).unwrap();
    let mut chart_builder = ChartBuilder::on(&drawing_area);
    chart_builder
        .margin(7)
        .set_left_and_bottom_label_area_size(20);

    let mut chart_context = chart_builder
        .build_cartesian_2d(0.0..10000.0, -1.1..1.1)
        .unwrap();
    chart_context.configure_mesh().draw().unwrap();

    chart_context.draw_series(LineSeries::new(coordinates, &RED))?;

    chart_context
        .configure_series_labels()
        .position(SeriesLabelPosition::UpperRight)
        .margin(20)
        .legend_area_size(5)
        .border_style(BLUE)
        .background_style(BLUE.mix(0.1))
        .label_font(("Calibri", 20))
        .draw()
        .unwrap();

    Ok(())
}

#[test]
fn test_plot(){
    // read wav from file
    let file_path = "testset/output1.wav";
    // let file_path = "test.wav";
    let mut reader = hound::WavReader::open(file_path).unwrap();
    let data: Vec<f32> = reader.samples::<f32>()
        .map(|s| s.unwrap())
        .collect();

    plot(data);
}

#[tokio::test]
async fn test_modulation() {
    // read data from testset/data.txt
    let mut file = File::open("testset/data.txt").unwrap();
    let mut data = String::new();
    file.read_to_string(&mut data).unwrap();
    let data = data
        .chars()
        .map(|c| c.to_digit(10).unwrap() as u8)
        .collect::<Vec<u8>>();

    // modulation
    let sample_rate = 48000;
    let carrier_freq = 1000;
    let mut modulator = Modulator::new(vec![carrier_freq], sample_rate, false);
    let modulated_signal = modulator.modulate(&data, 0);

    // show figure of the modulated_signal: Vec<f32>
    plot(modulated_signal.clone()).unwrap();

    // send
    modulator
        .send_bits(
            utils::read_data_2_compressed_u8(data.clone()),
            data.len() as isize,
        )
        .await;
}

#[tokio::test]
async fn test_demodulation_detect_windowshift() {
    let sample_rate = 48000;
    let carrier_freq = 1000;

    let normal = Normal::new(0.0, 0.3).unwrap();
    let mut rng = thread_rng();

    let demodulator = Demodulation::new(vec![carrier_freq], 48000, false);

    let mut padding: Vec<f32> = (0..0).map(|_| rng.sample(&normal)).collect();
    let mut back_padding: Vec<f32> = (0..1).map(|_| rng.sample(&normal)).collect();

    let t = (0..(sample_rate / carrier_freq) * 2).map(|t| t as f32 / sample_rate as f32);

    let mut test_vec = t
        .map(|t| (2.0 * std::f32::consts::PI * t * carrier_freq as f32).sin() + rng.sample(&normal))
        .collect::<Vec<f32>>();

    padding.append(&mut test_vec);
    padding.append(&mut back_padding);

    let mut buffer = Vec::new();
    buffer.push(demodulator.detect_windowshift(&padding[..], 0.0, 0));

    println!("buffer: {:?}", buffer);
}

#[tokio::test]
async fn test_demodulation_detect_preamble(){
    let sample_rate = 48000;
    let carrier_freq = 1000;
    let demodulator = Demodulation::new(vec![carrier_freq], 48000, false);
    
    let padding_lock = demodulator.buffer.clone();
    
    
    
    let handle1 = task::spawn(
        async move{
            // let _normal = Normal::new(0.0, 0.4).unwrap();
            // let mut rng = thread_rng();
            let mut padding = padding_lock.lock().await;

            padding.push_back((0..20).map(|_| 0.0).collect());

            // let mut padding: Vec<f32> = (0..0).map(|_| rng.sample(&normal)).collect();
            let back_padding: Vec<f32> = (0..0).map(|_| 0.0).collect();

            let phase_base = (0..(sample_rate / carrier_freq)).map(|t| t as f32 / sample_rate as f32);
            let phase1 = phase_base.clone().map(|t| (2.0 * std::f32::consts::PI * t * carrier_freq as f32).sin()).collect::<Vec<f32>>();
            let phase0 = phase_base.clone().map(|t| (2.0 * std::f32::consts::PI * t * carrier_freq as f32 + std::f32::consts::PI).sin()).collect::<Vec<f32>>();

            // padding.push_back(phase1.clone());
            // padding.push_back(phase1.clone());
            // padding.push_back(phase0.clone());
            // padding.push_back(phase0.clone());
            // padding.push_back(phase1.clone());
            drop(padding);
            println!("send 1st sequence");
            
            for i in 1..5{
                let mut padding = padding_lock.lock().await;
                padding.push_back(phase0.clone());
                padding.push_back(phase1.clone());
                drop(padding);
                let _ = sleep(Duration::from_millis(500));
                println!("send {}st sequence", i + 1);
            }

            let mut padding = padding_lock.lock().await;
            padding.push_back(phase0.clone());
            padding.push_back(phase1.clone());
            drop(padding);
            let _ = sleep(Duration::from_millis(500));

            let mut padding = padding_lock.lock().await;
            padding.push_back(back_padding);
            drop(padding);
        }
    );


    // join!(handle1);
    // let res = demodulator.detect_preamble(3);

    let handle2 = task::spawn(
        async move{demodulator.detect_preamble(4).await.unwrap();}
    );
    
    handle1.await.unwrap();
    handle2.await.unwrap();
    // println!("res: {:?}", res.await.expect("error"));
}


#[tokio::test]
async fn test_listen_directly(){
    let sample_rate = 48000;
    let carrier_freq = 1500;
    let mut demodulator = Demodulation::new(vec![carrier_freq], 48000, false);
    let mut modulation = Modulator::new(vec![carrier_freq], sample_rate, false);

    let data = vec![1, 0, 0, 1, 1, 0, 1, 0, 1, 0, 1, 1];
    // let data = [1; 2048].to_vec();
    let data = utils::read_data_2_compressed_u8(data);
    let buffer = demodulator.buffer.clone();

    let signal = modulation.send_bits_2_file(data.clone(), data.len() as isize, "test.wav").await;

    let mut buffer = buffer.lock().await;
    for vec in signal{
        buffer.push_back(vec);
    }

    drop(buffer);

    demodulator.listening(1).await;

    // let mut buffer = Vec::new();
    // for vec in signal{
    //     // buffer.push_back(vec);
    //     buffer.extend(vec);
    // }
    
    // let mut recv_data: Vec<u8> = Vec::new();
    // let mut index = 384;
    // println!("sample buffer: {:?}", buffer[index..index + 48].to_vec());
    // while index < buffer.len(){
    //     recv_data.push(demodulator.detect_windowshift(&buffer, 12.0, index).unwrap().received_bit);
    //     index += 48;
    // }

    // let recv_data = utils::read_data_2_compressed_u8(recv_data);
    // println!("len of recv_data: {}", recv_data.len());
    // println!("recv_data: {:?}", recv_data);
}

#[tokio::test]
async fn test_listening()
{
    let sample_rate = 48000;
    let carrier_freq = 1000;
    let mut demodulator = Demodulation::new(vec![carrier_freq], sample_rate, false);

    demodulator.listening(5).await;
}

#[tokio::test]
async fn test_2_listening(){
    use std::collections::VecDeque;

    let sample_rate = 48000;
    let carrier_freq = 1500;
    let mut demodulator = Demodulation2::new(vec![carrier_freq], sample_rate, false, "test.txt");
    
    // read wav from file
    let file_path = "testset/output1.wav";
    // let file_path = "testset/send.wav";
    // let file_path = "test.wav";
    let mut reader = hound::WavReader::open(file_path).unwrap();
    let data: Vec<f32> = reader.samples::<f32>()
        .map(|s| s.unwrap())
        .collect();
    
    let mut test_data = VecDeque::new();
    let mut count = 0;
    for i in 0..data.len(){
        if count == 0{
            test_data.push_back(Vec::new());
        }
        test_data.back_mut().unwrap().push(data[i]);
        count += 1;
        if count == 640{
            count = 0;
        }
    }
    let mut debug_vec = vec![];

    let result = demodulator.listening(true, test_data, &mut debug_vec).await;

    plot(debug_vec).unwrap();
}

#[test]
fn test_iter(){

    let len = 48;

    let mut test = Vec::new();

    let sin = (0..len).map(|n| (2.0 * std::f64::consts::PI * 1000.0 * n as f64 / 48000.0).sin());

    for _ in 0..5{
        test.extend(sin.clone());
        test.extend(sin.clone().map(|x| -x));
    }

    let mut power = 0.0;
    let factor = 1.0 / 32.0;
    for i in test{
        power = power * (1.0 - factor) + i * factor;
    }

    println!("power: {:?}", power);
}
