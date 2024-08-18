use std::vec;

use crate::acoustic_modem::demodulation::{self, Demodulation};

#[test]
fn test_demodulation() {
    let sample_rate = 48000;
    let carrier_freq = 1000;

    let mut demodulator = Demodulation::new(vec![carrier_freq], 48000, false);

    let mut padding: Vec<f32> = Vec::from([0.0; 1]);

    let t = (0..(sample_rate/carrier_freq)).map(|t| t as f32 / sample_rate as f32);

    let mut test_vec = t.map(|t| (2.0 * std::f32::consts::PI * t * carrier_freq as f32).sin()).collect::<Vec<f32>>();

    padding.append(&mut test_vec);

    let mut buffer = Vec::new();
    buffer.push(demodulator.phase_dot_product(&padding[..48]).unwrap());

    println!("buffer: {:?}", buffer);
}