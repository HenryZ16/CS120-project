use crate::asio_stream::InputAudioStream;
use anyhow::Error;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{HostId, SampleRate, SupportedStreamConfig};
use std::collections::VecDeque;
use std::iter::Sum;
use std::vec;
// use futures::executor::block_on;
// use futures::SinkExt;

// use super::phy_frame;

// const SAMPLE_RATE: u32 = 48000;

pub struct Demodulation{
    carrier_freq: Vec<u32>,
    enable_ofdm: bool,
    input_stream: InputAudioStream,
    config: SupportedStreamConfig,
    buffer: VecDeque<Vec<f32>>,
    ref_signal: Vec<Vec<f32>>,
}

impl Demodulation{
    pub fn new(carrier_freq: Vec<u32>, sample_rate: u32, enable_ofdm: bool) -> Self{
        let host = cpal::host_from_id(HostId::Asio).expect("failed to initialise ASIO host");
        let device = host.input_devices().expect("failed to find input device");
        let device = device
            .into_iter()
            .next()
            .expect("no input device available");
        println!("Input device: {:?}", device.name().unwrap());

        let default_config = device.default_input_config().unwrap();
        let config = SupportedStreamConfig::new(
            1,                       // mono
            SampleRate(sample_rate), // sample rate
            default_config.buffer_size().clone(),
            default_config.sample_format(),
        );

        let input_stream = InputAudioStream::new(&device, config.clone());

        let mut ref_signal = Vec::new();

        for i in 0..carrier_freq.len(){
            let ref_sin = (0..config.sample_rate().0 / *carrier_freq.get(i).unwrap() as u32).map(|x| (2.0 * std::f32::consts::PI * x as f32 / config.sample_rate().0 as f32).sin()).collect::<Vec<f32>>();
            ref_signal.push(ref_sin);
        }

        Demodulation{
            carrier_freq,
            enable_ofdm,
            input_stream,
            config,
            buffer: VecDeque::new(),
            ref_signal,
        }
    }

    // input length should be at least the length of the longest reference signal
    // output length is the number of carrier frequencies, each bit is represented by 1.0 or 0.0
    pub fn phase_demodulate(&self, input: &[f32]) -> Result<Vec<u8>, Error>{
        let input_len = input.len();

        if input_len < self.ref_signal.get(0).unwrap().len(){
            return Err(Error::msg("Input length is too short"));
        }

        let mut output = Vec::new();

        for i in 0..self.carrier_freq.len(){
            let mut dot_product = 0.0;
            let mut ref_signal_iter = self.ref_signal.get(i).unwrap().into_iter();
            for j in 0..input_len{
                dot_product += input[j] * ref_signal_iter.next().unwrap();
            }

            output.push(
                if dot_product > 0.0{
                    1
                }else{
                    0
                }
            );
        }

        Ok(output)
    }
}