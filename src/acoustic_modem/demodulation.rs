use crate::acoustic_modem::phy_frame::{self, FRAME_LENGTH_LENGTH_REDUNDANCY};
use crate::asio_stream::InputAudioStream;
use crate::utils::read_data_2_compressed_u8;
use anyhow::Error;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{SampleRate, SupportedStreamConfig, Device};
use futures::StreamExt;
use num_traits::pow;
use plotters::data;
use tokio::select;
use std::collections::VecDeque;
use std::io::Write;
use std::vec;
use tokio::{sync::{Mutex, oneshot}, time::{timeout, Duration}};
use std::sync::Arc;
use std::fs::File;
use std::ops::{Add, Mul};

use super::modulation::{self, Modulator};

struct InputStreamConfig{
    config: SupportedStreamConfig,
    device: Device,
}

impl InputStreamConfig{
    fn new(config: SupportedStreamConfig, device: Device) -> Self{
        InputStreamConfig{
            config,
            device,
        }
    }

    fn create_input_stream(&self) -> InputAudioStream{

        // println!("create input stream");
        // println!("config: {:?}", self.config);
        // println!("device: {:?}", self.device.name());

        InputAudioStream::new(&self.device, self.config.clone())
    }
}

struct DemodulationConfig{
    carrier_freq: Vec<u32>,
    sample_rate: u32,
    ref_signal: Vec<Vec<f32>>,
    ref_signal_len: Vec<usize>,
    preamble_len: usize,
    preamble: Vec<f32>,
}

unsafe impl Send for DemodulationConfig{}
unsafe impl Sync for DemodulationConfig{}

impl DemodulationConfig{
    fn new(carrier_freq: Vec<u32>, sample_rate: u32, ref_signal: Vec<Vec<f32>>, ref_signal_len: Vec<usize>) -> Self{
        let preamble = phy_frame::gen_preamble(sample_rate);
        println!("preamble len: {}", preamble.len());
        DemodulationConfig{
            carrier_freq,
            sample_rate,
            ref_signal,
            ref_signal_len,
            preamble_len: preamble.len(),
            preamble
        }
    }
}

#[derive(PartialEq, Debug)]
enum DemodulationState{
    DetectPreamble,
    RecvFrame,
    Stop,
}

impl DemodulationState{
    pub fn next(&self) -> Self{
        // println!("switched");

        match self{
            DemodulationState::DetectPreamble => DemodulationState::RecvFrame,
            DemodulationState::RecvFrame => DemodulationState::Stop,
            DemodulationState::Stop => DemodulationState::Stop,
        }
    }

    pub fn return_detect_preamble(&self) -> Self{
        DemodulationState::DetectPreamble
    }
}

pub fn dot_product(input: &[f32], ref_signal: &[f32]) -> f32{
    if input.len() != ref_signal.len(){
        panic!("Input length is not equal to reference signal length");
    }

    dot_product_iter(input.iter(), ref_signal.iter()) 
}

pub fn smooth(input: &[f32], window_size: i32) -> Vec<f32>{
    let mut smoothed_input = Vec::new();

    for i in 0..input.len(){
        let mut sum = 0.0;
        for j in i as i32 - window_size/2..i as i32 + window_size/2{
            if j < 0{
                sum += input[(j + input.len() as i32) as usize];
            }
            else if j >= input.len() as i32{
                sum += input[j as usize - input.len()];
            }
            else{
                sum += input[j as usize];
            }
        }
        smoothed_input.push(sum / window_size as f32);
    }

    smoothed_input
} 

pub fn exponential_smooth(input: &[f32], prev_signal: &mut f32, alpha: f32) -> Vec<f32>{
    let mut smoothed_input = Vec::new();

    for i in 0..input.len(){
        let smoothed = alpha * input[i] + (1.0 - alpha) * *prev_signal;
        smoothed_input.push(smoothed);
        *prev_signal = smoothed;
    }
    smoothed_input
}

pub fn dot_product_smooth(input: &[f32], ref_signal: &[f64], window_size: i32) -> f64{
    if input.len() != ref_signal.len(){
        panic!("Input length is not equal to reference signal length");
    }

    let mut smoothed_input = Vec::new();

    for i in 0..input.len(){
        let mut sum = 0.0;
        for j in i as i32 - window_size/2..i as i32 + window_size/2{
            if j < 0{
                sum += input[(j + input.len() as i32) as usize] as f64;
            }
            else if j >= input.len() as i32{
                sum += input[j as usize - input.len()] as f64;
            }
            else{
                sum += input[j as usize] as f64;
            }
        }
        smoothed_input.push(sum / window_size as f64);
    }

    dot_product_iter(smoothed_input.iter(), ref_signal.iter().map(|x| *x))
}

pub fn dot_product_exponential_smooth(input: &[f32], ref_signal: &[f64], prev_signal: &mut f32, alpha: f32) -> f64{
    if input.len() != ref_signal.len(){
        panic!("Input length is not equal to reference signal length");
    }

    let mut smoothed_input = exponential_smooth(input, prev_signal, alpha);

    dot_product_iter(smoothed_input.iter().map(|x| *x as f64), ref_signal.iter().map(|x| *x))
}

pub fn dot_product_iter<I, J, T, U, V>(iter1: I, iter2: J) -> V
where
    I: Iterator<Item = T>,
    J: Iterator<Item = U>,
    T: Mul<U, Output = V>,
    V: Add<Output = V> + Default,
{
    iter1.zip(iter2)
        .map(|(a, b)| a * b)
        .fold(V::default(), |acc, x| acc + x)
}

pub struct Demodulation2{
    input_config: InputStreamConfig,
    // pub buffer: VecDeque<Vec<f32>>,
    demodulate_config: DemodulationConfig,
    writer: File,
}

impl Demodulation2{
    pub fn new(carrier_freq: Vec<u32>, sample_rate: u32, output_file: &str, redundent_times: usize) -> Self{
        let mut redundent = 1;

        if redundent_times > 1{
            redundent = redundent_times;
        }

        // let host = cpal::host_from_id(HostId::Asio).expect("failed to initialise ASIO host");
        let host = cpal::default_host();
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

        let input_stream_config = InputStreamConfig::new(config, device);

        // sort carrier_freq in ascending order
        let mut carrier_freq = carrier_freq;
        carrier_freq.sort();

        let mut ref_signal = Vec::new();
        let mut ref_signal_len = Vec::new();

        for i in 0..carrier_freq.len(){
            let carrier = carrier_freq.get(i).unwrap();
            let ref_len = (sample_rate / *carrier) as usize * redundent_times;
            ref_signal_len.push(ref_len);
            let ref_sin = (0..ref_len).map(|t| (2.0 * std::f32::consts::PI * *carrier as f32 * (t as f32 / sample_rate as f32)).sin()).collect::<Vec<f32>>();
            ref_signal.push(ref_sin);
        }

        let demodulation_config = DemodulationConfig::new(carrier_freq, sample_rate, ref_signal, ref_signal_len); 

        let writer = File::create(output_file).unwrap();

        Demodulation2{
            input_config: input_stream_config,
            // buffer: VecDeque::new(),
            demodulate_config: demodulation_config,
            writer,
        }
    }


    pub async fn simple_listen(&mut self, write_to_file: bool, debug_vec: &mut Vec<f32>, data_len: usize) -> Vec<u8>{
        let data_len = data_len;

        let mut input_stream = self.input_config.create_input_stream();
        let demodulate_config = &self.demodulate_config;
        let alpha_check = 0.31;
        let mut prev = 0.0;

        let mut demodulate_state = DemodulationState::DetectPreamble;

        let mut avg_power = 0.0;
        let power_lim_preamble = 30.0;
        let factor = 1.0 / 64.0;

        let mut tmp_buffer: VecDeque<f32> = VecDeque::with_capacity(15 * demodulate_config.preamble_len.max(demodulate_config.ref_signal_len[0]));
        let mut tmp_buffer_len = tmp_buffer.len();

        let mut local_max = 0.0;
        let mut start_index = usize::MAX;

        let mut tmp_bits_data = Vec::with_capacity(data_len);
        // let mut res = vec![];

        while let Some(data) = input_stream.next().await{
            if demodulate_state == DemodulationState::Stop{
                break;
            }

            // debug_vec.extend(data.clone().iter());
            tmp_buffer_len += data.len();
            for i in data{
                avg_power = avg_power * (1.0 - factor) + i.abs() as f32 * factor;
                let processed_signal = i * alpha_check + prev * (1.0 - alpha_check);
                prev = i;
                tmp_buffer.push_back(processed_signal);
            }


            if demodulate_state == DemodulationState::DetectPreamble {
                if tmp_buffer_len <= demodulate_config.preamble_len{
                    continue;
                }
                tmp_buffer.make_contiguous();
                for i in 0..tmp_buffer_len - self.demodulate_config.preamble_len-1{
                    // let window = tmp_buffer.range(i..i+demodulate_config.preamble_len);
                    let window = &tmp_buffer.as_slices().0[i..i+demodulate_config.preamble_len];
                    let dot_product = dot_product(window, &demodulate_config.preamble);

                    if dot_product > avg_power * 2.0 && dot_product > local_max && dot_product > power_lim_preamble{
                        println!("detected");
                        local_max = dot_product;
                        start_index = i + 1;
                        // debug_vec.clear();
                        // debug_vec.extend(window);
                    }
                    else if start_index != usize::MAX && i - start_index > demodulate_config.preamble_len && local_max > power_lim_preamble{
                        local_max = 0.0;
                        start_index += demodulate_config.preamble_len - 1;
                        demodulate_state = demodulate_state.next();
                        println!("detected preamble");
                        break;
                    }
                }

            }
            
            if demodulate_state == DemodulationState::RecvFrame{
                if tmp_buffer_len - start_index <= demodulate_config.ref_signal_len[0]{
                    println!("tmp buffer is not long enough");
                    continue;
                }
                tmp_buffer.make_contiguous();

                while tmp_buffer_len - start_index >= demodulate_config.ref_signal_len[0] && tmp_bits_data.len() < data_len {
                    // let dot_product = range_dot_product_vec(tmp_buffer.range(start_index..start_index+demodulate_config.ref_signal_len[0]), &self.demodulate_config.ref_signal[0]);
                    let dot_product = dot_product(&tmp_buffer.as_slices().0[start_index..start_index+demodulate_config.ref_signal_len[0]], &self.demodulate_config.ref_signal[0]);
                    // debug_vec.extend(tmp_buffer.range(start_index..start_index + demodulate_config.ref_signal_len[0]));

                    start_index += demodulate_config.ref_signal_len[0];
                    
                    tmp_bits_data.push(if dot_product >= 0.0 {0} else {1});
                }
                println!("current recv length: {}", tmp_bits_data.len());
            }

            if tmp_bits_data.len() >= data_len{
                demodulate_state = demodulate_state.next();
            }
            
            let pop_times = if start_index == usize::MAX {tmp_buffer_len - demodulate_config.preamble_len+1} else {start_index};
    
            for i in 0..pop_times{
                tmp_buffer.pop_front();
            }
    
            start_index = if start_index == usize::MAX {usize::MAX} else {0};
            tmp_buffer_len = tmp_buffer.len();
        }

        if write_to_file{
            self.writer.write_all(&tmp_bits_data.clone().iter().map(|x| x + b'0').collect::<Vec<u8>>()).unwrap()
        }
        // println!("recv data: {:?}", tmp_bits_data);
        // println!("data: {:?}", tmp_bits_data);

        tmp_bits_data
    }

    pub async fn listen_frame(&mut self, write_to_file: bool, data_len: usize, redundent_times: usize) -> Vec<u8>{
        let mut redundent = 1;
        if redundent_times > 1{
            redundent = redundent_times;
        }

        let data_len = data_len;

        let mut input_stream = self.input_config.create_input_stream();
        let demodulate_config = &self.demodulate_config;
        let alpha_check = 0.31;
        let mut prev = 0.0;

        let mut demodulate_state = DemodulationState::DetectPreamble;

        let mut avg_power = 0.0;
        let power_lim_preamble = 30.0;
        let factor = 1.0 / 64.0;

        let mut tmp_buffer: VecDeque<f32> = VecDeque::with_capacity(15 * demodulate_config.preamble_len.max(demodulate_config.ref_signal_len[0]));
        let mut tmp_buffer_len = tmp_buffer.len();

        let mut local_max = 0.0;
        let mut start_index = usize::MAX;

        let mut tmp_bits_data = Vec::with_capacity(data_len);

        while let Some(data) = input_stream.next().await{
            if demodulate_state == DemodulationState::Stop{
                break;
            } 
            tmp_buffer_len += data.len();
            for i in data{
                avg_power = avg_power * (1.0 - factor) + i.abs() as f32 * factor;
                let processed_signal = i * alpha_check + prev * (1.0 - alpha_check);
                prev = i;
                tmp_buffer.push_back(processed_signal);
            }


            if demodulate_state == DemodulationState::DetectPreamble {
                if tmp_buffer_len <= demodulate_config.preamble_len{
                    continue;
                }
                tmp_buffer.make_contiguous();
                for i in 0..tmp_buffer_len - self.demodulate_config.preamble_len-1{
                    // let window = tmp_buffer.range(i..i+demodulate_config.preamble_len);
                    let window = &tmp_buffer.as_slices().0[i..i+demodulate_config.preamble_len];
                    let dot_product = dot_product(window, &demodulate_config.preamble);

                    if dot_product > avg_power * 2.0 && dot_product > local_max && dot_product > power_lim_preamble{
                        println!("detected");
                        local_max = dot_product;
                        start_index = i + 1;
                    }
                    else if start_index != usize::MAX && i - start_index > demodulate_config.preamble_len && local_max > power_lim_preamble{
                        local_max = 0.0;
                        start_index += demodulate_config.preamble_len - 1;
                        demodulate_state = demodulate_state.next();
                        println!("detected preamble");
                        break;
                    }
                }

            }
            
            if demodulate_state == DemodulationState::RecvFrame{
                if tmp_buffer_len - start_index <= demodulate_config.ref_signal_len[0]{
                    println!("tmp buffer is not long enough");
                    continue;
                }
                tmp_buffer.make_contiguous();

                while tmp_buffer_len - start_index >= demodulate_config.ref_signal_len[0] && tmp_bits_data.len() < data_len {
                    let dot_product = dot_product(&tmp_buffer.as_slices().0[start_index..start_index+demodulate_config.ref_signal_len[0]], &self.demodulate_config.ref_signal[0]);

                    start_index += demodulate_config.ref_signal_len[0];
                    
                    tmp_bits_data.push(if dot_product >= 0.0 {0} else {1});
                }
                println!("current recv length: {}", tmp_bits_data.len());
            }

            if tmp_bits_data.len() >= data_len{
                demodulate_state = demodulate_state.next();
            }
            
            let pop_times = if start_index == usize::MAX {tmp_buffer_len - demodulate_config.preamble_len+1} else {start_index};
    
            for i in 0..pop_times{
                tmp_buffer.pop_front();
            }
    
            start_index = if start_index == usize::MAX {usize::MAX} else {0};
            tmp_buffer_len = tmp_buffer.len();
        }

        if write_to_file{
            self.writer.write_all(&tmp_bits_data.clone().iter().map(|x| x + b'0').collect::<Vec<u8>>()).unwrap()
        }
        // println!("recv data: {:?}", tmp_bits_data);
        // println!("data: {:?}", tmp_bits_data);
        let mut actual_data_len = 0;
        let mut one_number = 0;
        let mut count = 0;
        for i in phy_frame::FRAME_PREAMBLE_LENGTH..phy_frame::FRAME_PREAMBLE_LENGTH+phy_frame::frame_length_length(){
            count += 1;

            if tmp_bits_data[i] == 1{
                one_number += 1;
            }

            if count == redundent{
                println!("one_number: {}", one_number);
                actual_data_len <<= 1;
                if one_number > redundent / 2{
                    actual_data_len += 1
                }

                one_number = 0;
                count = 0;
            }
        }

        println!("data len: {}", actual_data_len);
        read_data_2_compressed_u8(tmp_bits_data)
    }
}

fn range_dot_product_vec(range: std::collections::vec_deque::Iter<'_, f32>, ref_vec: &Vec<f32>) -> f32{
    dot_product_iter(range, ref_vec.iter().map(|x| *x))
}