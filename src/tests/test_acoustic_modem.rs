use std::fs::File;
use std::io::{Read, Write};
use std::time::Duration;
use std::vec;

use crate::acoustic_modem::demodulation::{self, Demodulation2};
use crate::acoustic_modem::modulation::Modulator;
use crate::acoustic_modem::{modulation, phy_frame};
use crate::utils::{self, read_data_2_compressed_u8};
use plotters::prelude::*;
use tokio::time;

use hound::{WavSpec, WavWriter};

fn plot(modulated_signal: Vec<f32>, filename: &str) -> Result<(), Box<dyn std::error::Error>> {
    // get the first 100000 samples
    let mut coordinates = vec![];
    for (i, sample) in modulated_signal.iter().enumerate() {
        if i > 100000 {
            break;
        }
        coordinates.push((i as f64, *sample as f64));
    }

    let drawing_area =
        SVGBackend::new(filename, (3000, 200)).into_drawing_area();
    drawing_area.fill(&WHITE).unwrap();
    let mut chart_builder = ChartBuilder::on(&drawing_area);
    chart_builder
        .margin(7)
        .set_left_and_bottom_label_area_size(20);

    let mut chart_context = chart_builder
        .build_cartesian_2d(0.0..25000.0, -1.1..1.1)
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

fn file_plot(input_file: &str, output_file: &str) {
    // read wav from file
    let file_path = input_file;
    // let file_path = "test.wav";
    let mut reader = hound::WavReader::open(file_path).unwrap();
    let data: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap()).collect();

    plot(data, output_file).unwrap();
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
    plot(modulated_signal.clone(), "testset/modulated_data_wave.svg").unwrap();

    // send
    modulator
        .send_bits(
            utils::read_data_2_compressed_u8(data.clone()),
            data.len() as isize,
        )
        .await;
}

#[test]
fn test_plot_wav() {
    let mut reader = hound::WavReader::open("test.wav").unwrap();
    let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap()).collect();

    // 创建绘图区域
    let root = SVGBackend::new("testset/ref_wave.svg", (3000, 200)).into_drawing_area();
    root.fill(&WHITE).unwrap();

    let mut chart = ChartBuilder::on(&root)
        .caption("Audio Waveform", ("sans-serif", 50).into_font())
        .margin(10)
        .x_label_area_size(30)
        .y_label_area_size(30)
        .build_cartesian_2d(0..samples.len(), -1.0..1.0)
        .unwrap();

    chart.configure_mesh().draw().unwrap();

    // 绘制波形图
    chart
        .draw_series(LineSeries::new(
            samples.iter().enumerate().map(|(x, y)| (x, *y as f64)),
            &BLUE,
        ))
        .unwrap();

    // 保存图像
    root.present().unwrap();
}

const CARRIER: u32 = 1200;
const LEN: usize = phy_frame::FRAME_PAYLOAD_LENGTH;
const REDUNDENT: usize = modulation::REDUNDANT_PERIODS;
const PADDING: usize = 0;
static CONFIG: [u32; 3] = [CARRIER, 6000, 1];

#[test]
fn test_simple_gen() {
    let carrier = CARRIER;
    let sample_rate = 48000;
    let simple_frame = phy_frame::SimpleFrame::new(carrier, LEN);

    let output_wav = simple_frame.into_audio(REDUNDENT, PADDING);

    plot(output_wav.clone(), "output_wav.svg").unwrap();

    // file write use
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = WavWriter::create("test_simple.wav", spec).unwrap();
    for sample in output_wav {
        writer.write_sample(sample).unwrap();
    }
}

#[tokio::test]
async fn test_simple_listen() {
    let mut demodulator = Demodulation2::new(CONFIG.into(), 48000, "output.txt", REDUNDENT);

    let mut debug_vec = vec![];

    use std::fs::File;
    use std::io::BufReader;

    // let reader = File::open("ref_signal.txt").unwrap();
    // let reader = BufReader::new(reader);
    // let mut ref_data = vec![];
    // for data in reader.bytes() {
    //     ref_data.push(data.unwrap() - b'0');
    // }

    // println!("ref: {:?}", ref_data);
    // loop {
        let res = demodulator.simple_listen(true, &mut debug_vec, LEN, PADDING).await;
        // let mut diff_num = 0;
        // for i in 0..ref_data.len() {
        //     if ref_data[i] != res[i] {
        //         diff_num += 1;
        //     }
        // }

        // println!("debug vec: {:?}", debug_vec);
        // plot(debug_vec, "recv_wav.svg").unwrap();
        println!("res: {:?}", res);
        // println!("error percent: {}", diff_num as f32 / ref_data.len() as f32);
    // }
}

#[tokio::test]
async fn test_frame_gen() {
    let sample_rate = 48000;
    let carrier = CARRIER;
    let mut modulation = Modulator::new(CONFIG.into(), sample_rate, false);

    // let data = vec![0,1,1,0,1,0,0,1,0,1];
    let mut file = File::open("testset/data.txt").unwrap();
    let mut data = String::new();
    file.read_to_string(&mut data).unwrap();
    let data = data
        .chars()
        .map(|c| c.to_digit(10).unwrap() as u8)
        .collect::<Vec<u8>>();

    println!("readed data: {:?}", data);
    let data_len = data.len() as isize;
    let data = read_data_2_compressed_u8(data);

    let _signal = modulation
        .send_bits_2_file(data, data_len, "test.wav")
        .await;

    file_plot("test.wav", "output_wav.svg");
}

#[tokio::test]
async fn test_seconds_listening() {
    let mut demodulator = Demodulation2::new(
        CONFIG.into(),
        48000,
        "output.txt",
        modulation::REDUNDANT_PERIODS,
    );

    let mut decoded_data = vec![];
    let mut debug_vec = vec![];
    let handle = demodulator.listening(true, phy_frame::FRAME_LENGTH_LENGTH + phy_frame::MAX_FRAME_DATA_LENGTH, &mut decoded_data, &mut debug_vec, vec![]);
    let handle = time::timeout(Duration::from_secs(20), handle);
    handle.await.unwrap();
    plot(debug_vec, "recv_wav.svg").unwrap();

    // println!("received data: {:?}", decoded_data);
}

#[tokio::test]
async fn test_ofdm_gen() {
    let mut modulation = Modulator::new(CONFIG.into(), 48000, true);

    // let data = vec![0,1,1,0,1,0,0,1,0,1];
    let mut file = File::open("testset/data.txt").unwrap();
    let mut data = String::new();
    file.read_to_string(&mut data).unwrap();
    let data = data
        .chars()
        .map(|c| c.to_digit(10).unwrap() as u8)
        .collect::<Vec<u8>>();

    println!("readed data: {:?}", data);
    let data_len = data.len() as isize;
    let data = read_data_2_compressed_u8(data);

    let _signal = modulation
        .send_bits_2_file(data, data_len, "test.wav")
        .await;

    file_plot("test.wav", "output_wav.svg");
}
fn read_wav_to_array(file_path: &str) -> Vec<f32> {
    let mut reader = hound::WavReader::open(file_path).unwrap();
    let spec = reader.spec();
    let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap()).collect();
    samples
}

#[tokio::test]
async fn test_ofdm_listen() {
    let mut demodulator = Demodulation2::new(
        CONFIG.into(), 48000, "output.txt", modulation::REDUNDANT_PERIODS);
    
    let mut decoded_data = vec![];
    let mut debug_vec = vec![];
    let wav_data = vec![read_wav_to_array("test.wav"), vec![0.0, 0.0], vec![]];
    println!("wav_data len: {}", wav_data[0].len());
    let handle = demodulator.listening(true, phy_frame::FRAME_LENGTH_LENGTH_NO_ENCODING + phy_frame::MAX_FRAME_DATA_LENGTH, &mut decoded_data, &mut debug_vec, wav_data);
    let handle = time::timeout(Duration::from_secs(5), handle);
    handle.await.unwrap_err();
    let mut writer = File::create("wav_data.txt").unwrap();
    println!("debug vec len: {}", debug_vec.len());
    for sample in &debug_vec{
        writeln!(writer, "{}", sample).unwrap();
    }
    // plot(debug_vec, "recv_wav.svg").unwrap();   
}