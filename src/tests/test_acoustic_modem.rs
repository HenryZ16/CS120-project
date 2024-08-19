use std::fs::File;
use std::io::Read;
use std::vec;

use crate::acoustic_modem::demodulation::{self, Demodulation};
use crate::acoustic_modem::modulation::Modulator;
use crate::utils;
use plotters::prelude::*;
use rand::thread_rng;
use rand::Rng;
use rand_distr::Normal;

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

// #[test]
// fn test_demodulation() {
//     let sample_rate = 48000;
//     let carrier_freq = 1000;

//     let normal = Normal::new(0.0, 0.2).unwrap();
//     let mut rng = thread_rng();

//     let mut demodulator = Demodulation::new(vec![carrier_freq], 48000, false);

//     let mut padding: Vec<f32> = (0..12).map(|_| rng.sample(&normal)).collect();
//     let mut back_padding: Vec<f32> = (0..10).map(|_| rng.sample(&normal)).collect();

//     let t = (0..(sample_rate / carrier_freq) * 2).map(|t| t as f32 / sample_rate as f32);

//     let mut test_vec = t
//         .map(|t| (2.0 * std::f32::consts::PI * t * carrier_freq as f32).sin() + rng.sample(&normal))
//         .collect::<Vec<f32>>();

//     padding.append(&mut test_vec);
//     padding.append(&mut back_padding);

//     let mut buffer = Vec::new();
//     buffer.push(demodulator.detect_windowshift(&padding));

//     println!("buffer: {:?}", buffer);
// }

// #[test]
// fn test_gen_vec(){
//     use crate::asio_stream;

//     let input_stream = asio_stream::InputAudioStream::new(&asio_stream::get_device(0), asio_stream::get_config(0, 48000));
// }
