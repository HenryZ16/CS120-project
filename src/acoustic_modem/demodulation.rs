use crate::acoustic_modem::phy_frame;
use crate::asio_stream::InputAudioStream;
use anyhow::Error;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{SampleRate, SupportedStreamConfig, Device};
use futures::StreamExt;
use num_traits::pow;
use plotters::data;
use tokio::select;
use std::collections::VecDeque;
use std::io::Write;
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

pub fn dot_product(input: &[f32], ref_signal: &[f64]) -> f64{
    if input.len() != ref_signal.len(){
        panic!("Input length is not equal to reference signal length");
    }

    dot_product_iter(input.iter().map(|x| *x as f64), ref_signal.iter().map(|x| *x)) 
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
    pub fn new(carrier_freq: Vec<u32>, sample_rate: u32, output_file: &str) -> Self{
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
        let mut preamble = Vec::new();

        for i in 0..carrier_freq.len(){
            let carrier = carrier_freq.get(i).unwrap();
            let ref_len = (sample_rate / *carrier) as usize;
            ref_signal_len.push(ref_len);
            let ref_sin = (0..ref_len).map(|t| (2.0 * std::f32::consts::PI * *carrier as f32 * (t as f32 / sample_rate as f32)).sin()).collect::<Vec<f32>>();
            for _ in 0..phy_frame::FRAME_PREAMBLE_LENGTH/2{
                preamble.extend(ref_sin.iter());
                preamble.extend(ref_sin.iter().map(|x| -*x));
            }
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

    // pub async fn listening(&mut self, write_to_file: bool, data: VecDeque<Vec<f32>>, debug_vec: &mut Vec<f32>) -> Vec<u8>{
    //     let mut input_stream = self.input_config.create_input_stream();
    //     let demodulate_config = &self.demodulate_config;
    //     let window_size = 10;
    //     let alpha_data = 0.3;
    //     let alpha_check = 0.7;
    //     let mut prev = 0.0;

    //     // let mut debug_vec = Vec::new();

    //     let mut demodulate_state = DemodulationState::DetectPreamble;

    //     let mut avg_power = 0.0;
    //     let power_lim_preamble = 10.0;
    //     // let power_lim_
    //     let factor = 1.0 / 64.0;

    //     let mut tmp_buffer: VecDeque<f32> = VecDeque::new();
    //     let mut tmp_buffer_len = tmp_buffer.len();

    //     let mut local_max = 0.0;
    //     let mut start_index = usize::MAX;

    //     let mut result = Vec::new();
    //     let mut tmp_bits_data: Vec<u8> = Vec::new();

    //     while let Some(data) = input_stream.next().await{
    //     // for data in data{
    //         if demodulate_state == DemodulationState::Stop {
    //             break;
    //         }
    //         debug_vec.extend(data.clone().iter());
    //         tmp_buffer_len += data.len();
    //         for i in data{
    //             avg_power = avg_power * (1.0 - factor) + i.abs() as f64 * factor;
    //             tmp_buffer.push_back(i);
    //         }
            
    //         if demodulate_state == DemodulationState::DetectPreamble{
    //             if tmp_buffer_len <= demodulate_config.preamble_len{
    //                 continue;
    //             }

    //             for i in 0..tmp_buffer_len - demodulate_config.preamble_len{
    //             // for i in 0..1{
    //                 let window = tmp_buffer.range(i..i + demodulate_config.preamble_len);

    //                 // println!("detect data: {:?}", window.clone().map(|x| *x).collect::<Vec<f32>>().as_slice());
    //                 // println!("ref signal: {:?}", demodulate_config.preamble);

    //                 // println!("preamble len: {:?}", demodulate_config.preamble_len);
    //                 // println!("tmp buffer len: {:?}", tmp_buffer_len);

    //                 // let dot_product = dot_product_exponential_smooth(window.clone().map(|x| *x).collect::<Vec<f32>>().as_slice(), 
    //                 //                                                       &demodulate_config.preamble.iter().map(|x| *x as f64).collect::<Vec<f64>>().as_slice(),
    //                 //                                                     &mut prev, alpha_check);

    //                 let dot_product = dot_product(window.clone().map(|x| *x).collect::<Vec<f32>>().as_slice(), 
    //                                                                       &demodulate_config.preamble.iter().map(|x| *x as f64).collect::<Vec<f64>>().as_slice());

    //                 // println!("dot product: {:?}, local max: {:?}", dot_product, local_max);
                    
    //                 if dot_product > avg_power * 5.0 && dot_product > local_max && dot_product > power_lim_preamble{
    //                     local_max = dot_product;
    //                     start_index = i + 1;
    //                     debug_vec.clear();
    //                     debug_vec.extend(window);
    //                 }
    //                 else if start_index != usize::MAX && i - start_index > demodulate_config.preamble_len / 2 && local_max > power_lim_preamble {
    //                     println!("local max: {}, average power: {}", local_max, avg_power);
    //                     local_max = 0.0;
    //                     start_index += demodulate_config.preamble_len - 1;
    //                     demodulate_state = demodulate_state.next();

    //                     break;
    //                 }
    //             }
    //         }

    //         else if demodulate_state == DemodulationState::Check{
    //             if tmp_buffer_len <= demodulate_config.check_len{
    //                 continue;
    //             }
                
    //             start_index += demodulate_config.check_len;
    //             tmp_bits_data.clear();
    //             tmp_bits_data.extend(vec![0,1,0,1,0,1,0,1,0,1]);
    //             demodulate_state = demodulate_state.next();

    //         }
    //         else if demodulate_state == DemodulationState::RecvFrame{
    //             if tmp_buffer_len <= demodulate_config.ref_signal_len[0]{
    //                 continue;
    //             }

    //             while tmp_buffer_len - start_index >= demodulate_config.ref_signal_len[0] && tmp_bits_data.len() < phy_frame::frame_length_length()+phy_frame::FRAME_PAYLOAD_LENGTH + phy_frame::FRAME_PREAMBLE_LENGTH{
    //                 // let dot_product = dot_product_smooth(tmp_buffer.range(start_index..start_index+demodulate_config.ref_signal_len[0]).map(|x| *x).collect::<Vec<f32>>().as_slice(), 
    //                 //                                           demodulate_config.ref_signal[0].iter().map(|x| *x).collect::<Vec<f64>>().as_slice(), 
    //                 //                                           window_size);

    //                 // let dot_product = dot_product_exponential_smooth(tmp_buffer.range(start_index..start_index+demodulate_config.ref_signal_len[0]).map(|x| *x).collect::<Vec<f32>>().as_slice(), 
    //                 //                                               demodulate_config.ref_signal[0].iter().map(|x| *x).collect::<Vec<f64>>().as_slice(), 
    //                 //                                               &mut prev, alpha_data);

    //                 // let dot_product = dot_product(tmp_buffer.range(start_index..start_index+demodulate_config.ref_signal_len[0]).map(|x| *x).collect::<Vec<f32>>().as_slice(), 
    //                 //                               demodulate_config.ref_signal[0].iter().map(|x| *x).collect::<Vec<f64>>().as_slice());

    //                 let dot_product = 0;

    //                 // println!("dot_product: {:?}", dot_product);
    //                 tmp_bits_data.push(if dot_product > 0.0 {0} else {1});

    //                 debug_vec.extend(tmp_buffer.range(start_index..start_index+demodulate_config.ref_signal_len[0]));
    //                 // debug_vec.extend(smooth(&tmp_buffer.range(start_index..start_index+demodulate_config.ref_signal_len[0]).map(|x| *x).collect::<Vec<f32>>()[..], window_size));
    //                 start_index += demodulate_config.ref_signal_len[0];
    //             }

    //             if tmp_bits_data.len() == phy_frame::frame_length_length()+phy_frame::FRAME_PAYLOAD_LENGTH + phy_frame::FRAME_PREAMBLE_LENGTH{
    //                 let mut loop_count = 0;
    //                 let mut ones_count = 0;
    //                 let mut data_len = 0;
    //                 for i in phy_frame::FRAME_PREAMBLE_LENGTH..phy_frame::frame_length_length()+phy_frame::FRAME_PREAMBLE_LENGTH{
    //                     ones_count += tmp_bits_data[i];
    //                     loop_count += 1;
    //                     if loop_count == 3{
    //                         data_len <<= 1;
    //                         if ones_count > 1{
    //                             data_len += 1;
    //                         }

    //                         ones_count = 0;
    //                         loop_count = 0;
    //                     }
    //                 }

    //                 let length = utils::read_data_2_compressed_u8(tmp_bits_data.iter().take(phy_frame::frame_length_length()+phy_frame::FRAME_PREAMBLE_LENGTH).cloned().collect());
    //                 println!("length: {:?}", length);
    //                 println!("actual len: {:?}", data_len);

    //                 let mut recv_data= utils::read_data_2_compressed_u8(tmp_bits_data.iter().skip(phy_frame::frame_length_length()+phy_frame::FRAME_PREAMBLE_LENGTH).cloned().collect());
    //                 println!("recv_data: {:?}", recv_data);

    //                 // construct the payload (to fit in the shard macro)
    //                 let mut i = 0;
    //                 let mut payload = phy_frame::PHYFrame::construct_payload_format(recv_data);

    //                 result.extend(&utils::read_compressed_u8_2_data(phy_frame::PHYFrame::payload_2_data(payload).unwrap())[..data_len]);
                    
    //                 if data_len == phy_frame::MAX_FRAME_DATA_LENGTH{
    //                     demodulate_state = demodulate_state.return_detect_preamble();
    //                     println!("return to detect preamble");
    //                 }
    //                 else{
    //                     demodulate_state = demodulate_state.next();
    //                     println!("stop receiving data");
    //                 }
    //                 // demodulate_state = DemodulationState::Stop;
    //             }
    //         }

    //         if start_index == usize::MAX{
    //             for i in 0..tmp_buffer_len - demodulate_config.preamble_len+1{
    //                 tmp_buffer.pop_front();
    //             }
    //         }
    //         else{
    //             for i in 0..start_index{
    //                 tmp_buffer.pop_front();
    //             }
    //             start_index = 0;
    //         }
    //         tmp_buffer_len = tmp_buffer.len();
    //     }

    //     if write_to_file{
    //         self.writer.write_all(result.clone().iter().map(|x| x + b'0').collect::<Vec<u8>>().as_slice()).unwrap();
    //     }

    //     result
    // }

    pub async fn simple_listen(&mut self, write_to_file: bool, debug_vec: &mut Vec<f32>, data_len: usize) -> Vec<u8>{
        let data_len = data_len;

        let mut input_stream = self.input_config.create_input_stream();
        let demodulate_config = &self.demodulate_config;
        let window_size = 10;
        let alpha_data = 0.3;
        let alpha_check = 0.31;
        let mut prev = 0.0;

        let mut demodulate_state = DemodulationState::DetectPreamble;

        let mut avg_power = 0.0;
        let power_lim_preamble = 15.0;
        let factor = 1.0 / 64.0;

        let mut tmp_buffer: VecDeque<f32> = VecDeque::new();
        let mut tmp_buffer_len = tmp_buffer.len();

        let mut local_max = 0.0;
        let mut start_index = usize::MAX;

        let mut tmp_bits_data: Vec<u8> = Vec::new();
        let mut res = vec![];

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
                for i in 0..tmp_buffer.len() - self.demodulate_config.preamble_len{
                    let window = tmp_buffer.range(i..i+demodulate_config.preamble_len);
                    let dot_product = range_dot_product_vec(window.clone(), &demodulate_config.preamble);

                    if dot_product > avg_power * 2.0 && dot_product > local_max && dot_product > power_lim_preamble{
                        // println!("detected");
                        local_max = dot_product;
                        start_index = i + 1;
                        debug_vec.clear();
                        debug_vec.extend(window);
                    }
                    else if start_index != usize::MAX && i - start_index > demodulate_config.preamble_len && local_max > power_lim_preamble{
                        local_max = 0.0;
                        start_index += demodulate_config.preamble_len - 1;
                        demodulate_state = demodulate_state.next();

                        break;
                    }
                }

            }
            
            if demodulate_state == DemodulationState::RecvFrame{
                if tmp_buffer_len - start_index <= demodulate_config.ref_signal_len[0]{
                    println!("tmp buffer is not long enough");
                    continue;
                }

                while tmp_buffer_len - start_index >= demodulate_config.ref_signal_len[0] && tmp_bits_data.len() < data_len {
                    let dot_product = range_dot_product_vec(tmp_buffer.range(start_index..start_index+demodulate_config.ref_signal_len[0]), &self.demodulate_config.ref_signal[0]);
                    
                    debug_vec.extend(tmp_buffer.range(start_index..start_index + demodulate_config.ref_signal_len[0]));

                    start_index += demodulate_config.ref_signal_len[0];

                    tmp_bits_data.push(
                        if dot_product > 0.0 {0}
                        else {1}
                    );
                }
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

        let mut count = 0;
        let mut numbers = 0;
        for &data in &tmp_bits_data{
            if data == 1{
                numbers += 1;
            }
            count += 1;
            if count == 1{
                count = 0;
                res.push((numbers >= 1) as u8);
                numbers = 0;
            }
        }

        if write_to_file{
            self.writer.write_all(&res.clone().iter().map(|x| x + b'0').collect::<Vec<u8>>()).unwrap()
        }
        println!("recv data: {:?}", tmp_bits_data);
        println!("data: {:?}", res);

        res
    }

    // fn has_detect_preamble(&self, detect_buffer: &VecDeque<f32>, local_max: &){
    //     for i in 0..detect_buffer.len() - self.demodulate_config.preamble_len{
    //         let window: std::collections::vec_deque::Iter<'_, f32> = detect_buffer.range(i..i+self.demodulate_config.preamble_len);
    //         let dot_product = range_dot_product_vec(window, &self.demodulate_config.preamble);


    //     }
    // }
}

fn range_dot_product_vec(range: std::collections::vec_deque::Iter<'_, f32>, ref_vec: &Vec<f32>) -> f32{
    dot_product_iter::<std::collections::vec_deque::Iter<'_, f32>, std::vec::IntoIter<f32>, &f32, f32, f32>(range, ref_vec.clone().into_iter())
}