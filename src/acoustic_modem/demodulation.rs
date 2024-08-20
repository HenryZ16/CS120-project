use crate::asio_stream::InputAudioStream;
use anyhow::Error;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{HostId, SampleRate, SupportedStreamConfig};
use futures::StreamExt;
use std::collections::VecDeque;
use std::env::consts;
// use futures::executor::block_on;
// use futures::SinkExt;

use super::phy_frame;

use tokio::{sync::Mutex, task, time::{timeout, Duration}};
use std::sync::Arc;

struct DemodulationConfig{
    carrier_freq: Vec<u32>,
    enable_ofdm: bool,
    ref_signal: Vec<Vec<f64>>,
    ref_signal_len: Vec<u32>,
}

unsafe impl Send for DemodulationConfig{}
unsafe impl Sync for DemodulationConfig{}

impl DemodulationConfig{
    fn new(carrier_freq: Vec<u32>, enable_ofdm: bool, ref_signal: Vec<Vec<f64>>, ref_signal_len: Vec<u32>) -> Self{
        DemodulationConfig{
            carrier_freq,
            enable_ofdm,
            ref_signal,
            ref_signal_len,
        }
    }
}

pub struct Demodulation{
    input_stream: InputAudioStream,
    buffer: Arc<Mutex<VecDeque<Vec<f32>>>>,
    // carrier_freq: Vec<u32>,
    // enable_ofdm: bool,
    // ref_signal: Vec<Vec<f64>>,
    // ref_signal_len: Vec<u32>,

    // preamble_state: PreambleState,

    config: DemodulationConfig,
}

// the return type of window shift detection
// in order to detect the alignment of the input signal
#[derive(Debug)]
pub struct AlignResult{
    align_index: u32,
    dot_product: f32,
}

#[derive(PartialEq)]
pub enum PreambleState{
    Waiting,
    First0,
    First1,
    Second0,
    Second1,
    Third0,
    Third1,
    Fourth0,
    Fourth1,
    Fifth0,
    ToRecv,
}

impl PreambleState {
    pub fn next(&self) -> Self{
        match self{
            PreambleState::Waiting => PreambleState::First0,
            PreambleState::First0 => PreambleState::First1,
            PreambleState::First1 => PreambleState::Second0,
            PreambleState::Second0 => PreambleState::Second1,
            PreambleState::Second1 => PreambleState::Third0,
            PreambleState::Third0 => PreambleState::Third1,
            PreambleState::Third1 => PreambleState::Fourth0,
            PreambleState::Fourth0 => PreambleState::Fourth1,
            PreambleState::Fourth1 => PreambleState::Fifth0,
            PreambleState::Fifth0 => PreambleState::ToRecv,
            PreambleState::ToRecv => PreambleState::ToRecv,
        }
    }

    pub fn back_waiting(&self) -> Self{
        PreambleState::Waiting
    }

    pub fn wait_zero(&self) -> bool{
        if *self == PreambleState::Waiting || *self == PreambleState::First1 || *self == PreambleState::Second1 || *self == PreambleState::Third1 || *self == PreambleState::Fourth1{
            true
        }
        else{
            false
        }
    }

    pub fn state_move(&mut self, input: u8){
        if self.wait_zero() && input == 0{
            *self = self.next();
        }
        else if !self.wait_zero() && input == 1{
            *self = self.next();
        }
        else{
            *self = self.back_waiting();
        }
    }
}

impl AlignResult{
    pub fn new() -> Self{
        AlignResult{
            align_index: std::u32::MAX,
            dot_product: 0.0,
        }
    }
}

unsafe impl Send for AlignResult{}
unsafe impl Sync for AlignResult{}
unsafe impl Send for Demodulation{}
unsafe impl Sync for Demodulation{}

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

        // sort carrier_freq in ascending order
        let mut carrier_freq = carrier_freq;
        carrier_freq.sort();

        let mut ref_signal = Vec::new();
        let mut ref_signal_len = Vec::new();

        for i in 0..carrier_freq.len(){
            let carrier = carrier_freq.get(i).unwrap();
            let ref_len = sample_rate / *carrier;
            ref_signal_len.push(ref_len);
            let ref_sin = (0..ref_len).map(|t| (2.0 * std::f64::consts::PI * *carrier as f64 * (t as f64 / sample_rate as f64)).sin()).collect::<Vec<f64>>();
            ref_signal.push(ref_sin);
        }

        let demodulation_config = DemodulationConfig::new(carrier_freq, enable_ofdm, ref_signal, ref_signal_len); 

        Demodulation{
            input_stream,
            buffer: Arc::new(Mutex::new(VecDeque::new())),
            config: demodulation_config,
        }
    }

    // input length should be at least the length of the longest reference signal
    // output is the dot product of the input signal and the reference signal
    // output length is the number of carrier frequencies
    pub fn phase_dot_product(&self, input: &[f32]) -> Result<Vec<f32>, Error>{
        let demodulation_config = &self.config;
        let input_len = input.len();        

        let mut output = Vec::new();

        for i in 0..demodulation_config.carrier_freq.len(){
            if input_len != *demodulation_config.ref_signal_len.get(i).unwrap() as usize{
                println!("input len: {:?}", input_len);
                println!("ref_signal len: {:?}", demodulation_config.ref_signal.get(i).unwrap().len());
                return Err(Error::msg("Input length is not equal to reference signal length"));
            }

            let mut dot_product: f64 = 0.0;
            let mut ref_signal_iter = demodulation_config.ref_signal.get(i).unwrap().into_iter();
            for j in 0..input_len{
                dot_product += input[j] as f64 * ref_signal_iter.next().unwrap();
            }
            
            // println!("dot_product: {:?}", dot_product);

            output.push(dot_product as f32);
        }

        Ok(output)
    }

    // find the index of the first maximum/minimum value in the input vector
    // detect frequency is the first carrier frequency
    // TODO: remove the half wave of input signal
    pub fn detect_windowshift(&self, input: &Vec<f32>, power_floor: f32) -> Result<AlignResult, Error>{
        let demodulation_config = &self.config;

        let power_floor: f32 = {
            if power_floor < 5.0{
                5.0
            }
            else{
                power_floor
            }
        };

        let input_len = input.len();

        let mut result = AlignResult::new();
        let mut prev_is_max = false;

        let detect_signal_len = *demodulation_config.ref_signal_len.get(0).unwrap() as usize;

        // println!("input_len: {:?}", input_len);
        // println!("detect_freq: {:?}", detect_freq);
        if input_len < detect_signal_len{
            return Err(Error::msg("Input length is less than reference signal length"));
        }

        for i in 0..(input_len - detect_signal_len + 1){
            let window_input = &input[i..(i + detect_signal_len)];
            let phase_product = self.phase_dot_product(window_input).unwrap()[0];
            // println!("phase_product: {:?}", phase_product);
            if phase_product.abs() > power_floor{
                if phase_product.abs() > result.dot_product.abs(){
                    result.align_index = i as u32;
                    result.dot_product = phase_product;
                    prev_is_max = true;
                }
                else if prev_is_max{
                    println!("result: {:?}", result);
                    break;
                }
            } 
        }
        
        if result.align_index != std::u32::MAX{
            return Ok(result);
        }

        Err(Error::msg("No alignment found"))
    }

    pub async fn detect_preamble(&self, time_limit: u64) -> Result<(), Error>{
        let error = 3.0;

        let duration = Duration::from_secs(time_limit);
        let demodulation_config = &self.config;
        let ref_signal_len = *demodulation_config.ref_signal_len.get(0).unwrap() as usize;
        match timeout(duration, async move{
            let mut last_align_result = AlignResult::new();
            let mut preamble_state = PreambleState::Waiting;
            let mut concat_buffer: Vec<f32> = Vec::new();
            let mut buffer_read_index = 0;
            let mut is_aligned = false;
            while preamble_state != PreambleState::ToRecv {
                let mut buffer = self.buffer.lock().await;
                if buffer.len() > 0{
                    let mut buffer_iter = buffer.iter();
                    for _ in 0..buffer_read_index{
                        buffer_iter.next();
                    }
                    for i in buffer_iter{
                        concat_buffer.extend(i);
                        buffer_read_index += 1;
                    }
                    if concat_buffer.len() < ref_signal_len as usize{
                        continue;
                    }

                    if !is_aligned{
                        let align_result = self.detect_windowshift(&concat_buffer, 5.0).unwrap();
                        if align_result.align_index != std::u32::MAX{
                            if last_align_result.align_index == std::u32::MAX{
                                last_align_result = align_result;
                            }
                            else if last_align_result.align_index != align_result.align_index{
                                if (last_align_result.dot_product - align_result.dot_product).abs() < last_align_result.dot_product / 3.0{
                                    is_aligned = true;
                                    preamble_state.state_move(
                                        if last_align_result.dot_product > 0.0{
                                            1
                                        }
                                        else {
                                            0
                                        }
                                    );
                                    preamble_state.state_move(
                                        if align_result.dot_product > 0.0{
                                            1
                                        }
                                        else {
                                            0
                                        }
                                    );
                                    last_align_result = align_result;
                                    
                                    buffer_read_index = 0;
                                    
                                    while buffer.get(0).unwrap().len() > last_align_result.align_index{
                                        let tmp_vec = buffer.pop_front().unwrap();
                                        last_align_result.align_index -= tmp_vec.len() as u32;
                                    }
                                }
                            }
                            else{
                                continue;
                            }
                        }
                        else{
                            continue;
                        }
                    }
                }

                // has aligned
                else{
                    

                }
            }
            Ok::<(), Error>(())
        }).await{
            Ok(_) => Ok(()),
            Err(_) => Err(Error::msg("Timeout")),
        }
    }
}