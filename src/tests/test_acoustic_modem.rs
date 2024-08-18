use std::vec;

use crate::acoustic_modem::demodulation::{self, Demodulation};
use rand::Rng;
use rand::thread_rng;
use rand_distr::Normal;

#[test]
fn test_demodulation() {
    let sample_rate = 48000;
    let carrier_freq = 1000;

    let normal = Normal::new(0.0, 0.2).unwrap();
    let mut rng = thread_rng();

    let mut demodulator = Demodulation::new(vec![carrier_freq], 48000, false);

    let mut padding: Vec<f32> = (0..12).map(|_| rng.sample(&normal)).collect();
    let mut back_padding: Vec<f32> = (0..10).map(|_| rng.sample(&normal)).collect();

    let t = (0..(sample_rate/carrier_freq)*2).map(|t| t as f32 / sample_rate as f32);

    let mut test_vec = t.map(|t| (2.0 * std::f32::consts::PI * t * carrier_freq as f32).sin() + rng.sample(&normal)).collect::<Vec<f32>>();

    padding.append(&mut test_vec);
    padding.append(&mut back_padding);

    let mut buffer = Vec::new();
    buffer.push(demodulator.detect_windowshift(&padding));

    println!("buffer: {:?}", buffer);
}

// #[test]
// fn test_gen_vec(){
//     use crate::asio_stream;

//     let input_stream = asio_stream::InputAudioStream::new(&asio_stream::get_device(0), asio_stream::get_config(0, 48000));
// }