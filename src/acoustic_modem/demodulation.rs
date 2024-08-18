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
    ref_signal_len: Vec<u32>,
}

struct AlignResult{
    phase: u8,
    align_index: u32,
}

impl AlignResult{
    pub fn new(phase: u8, align_index: u32) -> Self{
        AlignResult{
            phase,
            align_index,
        }
    }
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
        let mut ref_signal_len = Vec::new();

        for i in 0..carrier_freq.len(){
            let carrier = carrier_freq.get(i).unwrap();
            let ref_len = sample_rate / *carrier;
            ref_signal_len.push(ref_len);
            let ref_sin = (0..ref_len).map(|t| (2.0 * std::f32::consts::PI * *carrier as f32 * (t as f32 / sample_rate as f32)).sin()).collect::<Vec<f32>>();
            ref_signal.push(ref_sin);
        }

        Demodulation{
            carrier_freq,
            enable_ofdm,
            input_stream,
            config,
            buffer: VecDeque::new(),
            ref_signal,
            ref_signal_len,
        }
    }

    // input length should be at least the length of the longest reference signal
    // output is the dot product of the input signal and the reference signal
    // output length is the number of carrier frequencies
    pub fn phase_dot_product(&self, input: &[f32]) -> Result<Vec<f32>, Error>{
        let input_len = input.len();        

        let mut output = Vec::new();

        for i in 0..self.carrier_freq.len(){
            if input_len != self.ref_signal.get(i).unwrap().len(){
                println!("input len: {:?}", input_len);
                println!("ref_signal len: {:?}", self.ref_signal.get(i).unwrap().len());
                return Err(Error::msg("Input length is not equal to reference signal length"));
            }

            let mut dot_product = 0.0;
            let mut ref_signal_iter = self.ref_signal.get(i).unwrap().into_iter();
            // let ref_signal_vec = self.ref_signal.get(i).unwrap().clone();
            for j in 0..input_len{
                dot_product += input[j] * ref_signal_iter.next().unwrap();
            }
            
            println!("dot_product: {:?}", dot_product);

            output.push(dot_product);
        }

        Ok(output)
    }

    pub fn detect_windowshift(&self, input: Vec<f32>) -> Result<AlignResult, Error>{
        let input_len = input.len();
        Err(Error::msg("Not implemented"))
    }
}