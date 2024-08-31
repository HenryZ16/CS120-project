use crate::acoustic_modem::phy_frame::{self, PHYFrame};
use crate::asio_stream::InputAudioStream;
use crate::utils;
use anyhow::Error;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{SampleRate, SupportedStreamConfig, Device};
use futures::StreamExt;
use tokio::select;
use std::collections::VecDeque;
use std::io::Write;
use tokio::{sync::{Mutex, oneshot}, time::{timeout, Duration}};
use std::sync::Arc;
use std::fs::File;
use std::ops::{Add, Mul};

struct DemodulationConfig{
    carrier_freq: Vec<u32>,
    enable_ofdm: bool,
    ref_signal: Vec<Vec<f64>>,
    ref_signal_len: Vec<usize>,
    preamble_len: usize,
    preamble: Vec<f64>,
}

unsafe impl Send for DemodulationConfig{}
unsafe impl Sync for DemodulationConfig{}

impl DemodulationConfig{
    fn new(carrier_freq: Vec<u32>, enable_ofdm: bool, ref_signal: Vec<Vec<f64>>, ref_signal_len: Vec<usize>, preamble: Vec<f64>) -> Self{
        DemodulationConfig{
            carrier_freq,
            enable_ofdm,
            ref_signal,
            ref_signal_len,
            preamble_len: preamble.len(),
            preamble,
        }
    }
}

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

unsafe impl Send for InputStreamConfig{}
unsafe impl Sync for InputStreamConfig{}

// the return type of window shift detection
// in order to detect the alignment of the input signal
#[derive(Debug)]
pub struct AlignResult{
    align_index: usize,
    dot_product: f32,
    pub received_bit: u8,
}

#[derive(PartialEq, Debug)]
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
        
        println!("current state: {:?}", self);
    }
}

impl AlignResult{
    pub fn new() -> Self{
        AlignResult{
            align_index: std::usize::MAX,
            dot_product: 0.0,
            received_bit: 0,
        }
    }
    
    pub fn copy(other: &Self)->Self{
        AlignResult{
            align_index: other.align_index,
            dot_product: other.dot_product,
            received_bit: other.received_bit,
        }
    }
}

unsafe impl Send for AlignResult{}
unsafe impl Sync for AlignResult{}
pub struct Demodulation{
    input_config: InputStreamConfig,
    pub buffer: Arc<Mutex<VecDeque<Vec<f32>>>>,
    demodulate_config: DemodulationConfig,
}
unsafe impl Send for Demodulation{}
unsafe impl Sync for Demodulation{}

impl Demodulation{
    pub fn new(carrier_freq: Vec<u32>, sample_rate: u32, enable_ofdm: bool) -> Self{
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
        let preamble = Vec::new();

        for i in 0..carrier_freq.len(){
            let carrier = carrier_freq.get(i).unwrap();
            let ref_len = (sample_rate / *carrier) as usize;
            ref_signal_len.push(ref_len);
            let ref_sin = (0..ref_len).map(|t| - (2.0 * std::f64::consts::PI * *carrier as f64 * (t as f64 / sample_rate as f64)).sin()).collect::<Vec<f64>>();
            ref_signal.push(ref_sin);

        }

        let demodulation_config = DemodulationConfig::new(carrier_freq, enable_ofdm, ref_signal, ref_signal_len, preamble); 

        Demodulation{
            input_config: input_stream_config,
            buffer: Arc::new(Mutex::new(VecDeque::new())),
            demodulate_config: demodulation_config,
        }
    }

    // input length should be at least the length of the longest reference signal
    // output is the dot product of the input signal and the reference signal
    // output length is the number of carrier frequencies
    pub fn phase_dot_product(&self, input: &[f32]) -> Result<Vec<f32>, Error>{
        let demodulation_config = &self.demodulate_config;
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
    pub fn detect_windowshift(&self, input: &[f32], power_floor: f32, start_index: usize) -> Result<AlignResult, Error>{
        let demodulation_config = &self.demodulate_config;

        let power_floor: f32 = {
            if power_floor < 0.1{
                0.1
            }
            else{
                power_floor
            }
        };

        let input_len = input.len() - start_index;

        let mut result = AlignResult::new();
        let mut prev_is_max = false;

        let detect_signal_len = *demodulation_config.ref_signal_len.get(0).unwrap() as usize;

        // println!("input_len: {:?}", input_len);
        // println!("detect_freq: {:?}", detect_freq);
        if input_len < detect_signal_len{
            return Err(Error::msg("Input length is less than reference signal length"));
        }

        for i in start_index..(input_len - detect_signal_len + 1 + start_index){
            let window_input = &input[i..(i + detect_signal_len)];
            let phase_product = self.phase_dot_product(window_input).unwrap()[0];
            // println!("phase_product: {:?}", phase_product);
            if phase_product.abs() > power_floor{
                if phase_product.abs() > result.dot_product.abs(){
                    result.align_index = i;
                    result.received_bit = if phase_product > 0.0{
                        1
                    }
                    else{
                        0
                    };
                    result.dot_product = phase_product.abs();
                    prev_is_max = true;
                }
                else if prev_is_max{
                    // println!("result: {:?}", result);
                    break;
                }
            } 
        }
        
        if result.align_index != std::usize::MAX{
            // println!("result: {:?}", result);
            return Ok(result);
        }

        Err(Error::msg("No alignment found"))
    }

    pub async fn detect_preamble(&self, time_limit: u64) -> Result<AlignResult, Error>{
        let duration = Duration::from_secs(time_limit);
        // let duration = Duration::from_millis(100);
        let demodulation_config = &self.demodulate_config;
        let ref_signal_len = *demodulation_config.ref_signal_len.get(0).unwrap() as usize;
        match timeout(duration, async move{
            // println!("start detect preamble");

            let mut last_align_result = AlignResult::new();
            let mut preamble_state = PreambleState::Waiting;
            let mut concat_buffer: Vec<f32> = Vec::new();
            let mut buffer_read_index = 0;
            let mut is_aligned = false;
            while preamble_state != PreambleState::ToRecv {
                // println!("preamble_state: {:?}", preamble_state);

                let mut buffer = self.buffer.lock().await;
                // println!("get lock in detection, buffer len: {:?}", buffer.len());
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

                    let start = if last_align_result.align_index == std::usize::MAX{
                        0
                    }
                    else{
                        last_align_result.align_index + if !is_aligned{ref_signal_len / 6} else{0}
                    };

                    let align_result = self.detect_windowshift(&concat_buffer[..], last_align_result.dot_product / 3.0 * 2.0, start);
                    let mut align_result = match align_result{
                        Ok(result) => result,
                        Err(_) => {
                            last_align_result.align_index = concat_buffer.len() - ref_signal_len;
                            AlignResult::copy(&last_align_result)
                        }
                    };


                    if !is_aligned{
                        if last_align_result.align_index != align_result.align_index{
                            preamble_state.state_move(align_result.received_bit); 
                            if (last_align_result.dot_product - align_result.dot_product).abs() < (last_align_result.dot_product / 3.0){
                                if preamble_state == PreambleState::First1{
                                    is_aligned = true;
                                    println!("aligned");                         
                                }
                            }
                        }
                        
                        
                        last_align_result = align_result;
                    }
                    else{
                        loop{
                            preamble_state.state_move(align_result.received_bit);
                            if preamble_state != PreambleState::Waiting{
                                last_align_result = align_result;
                                if preamble_state != PreambleState::ToRecv && last_align_result.align_index + 2 * ref_signal_len < concat_buffer.len(){
                                    let tmp_res = self.detect_windowshift(&concat_buffer[..], last_align_result.dot_product * 2.0 / 3.0, last_align_result.align_index + ref_signal_len);
                                    match tmp_res{
                                        Ok(res) => {
                                            align_result = res;
                                        }
                                        Err(_) => {
                                            break;
                                        }
                                    }
                                }
                                else {
                                    break;
                                }
                            }
                            else {
                                is_aligned = false;
                                break;
                            }
                        }
                    }

                    if is_aligned{
                        last_align_result.align_index += ref_signal_len;
                    }

                    buffer_read_index = 0;
                    concat_buffer.clear();
                    while !buffer.is_empty() && (buffer.get(0).unwrap().len() <= last_align_result.align_index){
                        let tmp_vec = buffer.pop_front().unwrap();
                        last_align_result.align_index -= tmp_vec.len();
                    }   

                }

                // has aligned
            }
            // println!("have detected preamble, start receiving data");
            println!("last_align_result: {:?}", last_align_result);
            last_align_result
        }).await{
            Ok(last_align_result) => Ok(last_align_result),
            Err(_) => Err(Error::msg("Timeout")),
        }
    }


    pub async fn recv_frame(&self, align_result: &AlignResult) -> Result<Vec<u8>, Error>{
        let demodulate_config = &self.demodulate_config;
        let ref_signal_len = *demodulate_config.ref_signal_len.get(0).unwrap() as usize;
        let power_floor = align_result.dot_product / 3.0 * 2.0;
        let mut last_align_result = AlignResult::copy(align_result);

        let mut recv_buffer: Vec<u8> = vec![0,0];
        
        let mut concat_buffer: Vec<f32> = Vec::new();
        let mut buffer_read_index = 0;
        
        let mut length: usize = 0;
        let mut bits_num = 0;

        // let mut buffer = self.buffer.lock().await;
        // println!("buffer sample: {:?}", buffer.get(0).unwrap()[start_index..start_index + 48].to_vec());

        while bits_num < 30{
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

                let align_result = self.detect_windowshift(&concat_buffer[..], power_floor, last_align_result.align_index);
                let mut align_result = match align_result{
                    Ok(result) => result,
                    Err(_) => {
                        return Err(Error::msg("No contunuous data found"));
                    }
                };

                loop{
                    last_align_result = align_result;
                    recv_buffer.push(last_align_result.received_bit);
                    bits_num += 1;
                    if bits_num < 30 && last_align_result.align_index + 2 * ref_signal_len < concat_buffer.len(){
                        let tmp_res = self.detect_windowshift(&concat_buffer[..], power_floor, last_align_result.align_index + ref_signal_len);
                        match tmp_res{
                            Ok(res) => {
                                align_result = res;
                            }
                            Err(_) => {
                                break;
                            }
                        }
                    }
                    else {
                        break;
                    }
                }

                buffer_read_index = 0;
                concat_buffer.clear();
                while !buffer.is_empty() && (buffer.get(0).unwrap().len() <= last_align_result.align_index){
                    let tmp_vec = buffer.pop_front().unwrap();
                    last_align_result.align_index -= tmp_vec.len();
                }   
            }
        }

        last_align_result.align_index += ref_signal_len;

        for i in 0..10{
            let mut ones = 0;
            for j in 0..3{
                if recv_buffer[i * 3 + j + 2] == 1{
                    ones += 1;
                }
            }
            length = length * 2 + ones;
        }
        
        let recv_buffer = utils::read_data_2_compressed_u8(recv_buffer);
        println!("recv_buffer: {:?}", recv_buffer);
        println!("length: {:?}", length);
        let mut recv_buffer: Vec<u8> = Vec::new();

        let mut bits_num = 0;
        while bits_num < phy_frame::FRAME_PAYLOAD_LENGTH{
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

                let align_result = self.detect_windowshift(&concat_buffer[..], power_floor, last_align_result.align_index);
                let mut align_result = match align_result{
                    Ok(result) => result,
                    Err(_) => {
                        return Err(Error::msg("No contunuous data found"));
                    }
                };

                loop{
                    last_align_result = align_result;
                    recv_buffer.push(last_align_result.received_bit);
                    bits_num += 1;
                    if bits_num < phy_frame::FRAME_PAYLOAD_LENGTH && last_align_result.align_index + 2 * ref_signal_len < concat_buffer.len(){
                        let tmp_res = self.detect_windowshift(&concat_buffer[..], power_floor, last_align_result.align_index + ref_signal_len);
                        match tmp_res{
                            Ok(res) => {
                                align_result = res;
                            }
                            Err(_) => {
                                break;
                            }
                        }
                    }
                    else {
                        break;
                    }
                }

                buffer_read_index = 0;
                concat_buffer.clear();
                while !buffer.is_empty() && (buffer.get(0).unwrap().len() <= last_align_result.align_index){
                    let tmp_vec = buffer.pop_front().unwrap();
                    last_align_result.align_index -= tmp_vec.len();
                }
            }
        }
        println!("recv_buffer: {:?}", utils::read_data_2_compressed_u8(recv_buffer.clone()));
        
        // let recv_buffer = phy_frame::PHYFrame::data_2_payload(utils::read_data_2_compressed_u8(recv_buffer), length).unwrap();
        // let res = phy_frame::PHYFrame::payload_2_data(recv_buffer).unwrap();


        Ok(recv_buffer)
    }

    pub async fn listening(&mut self, time_limit: u64){
        println!("start recording");

        let mut buffer_input = self.buffer.clone();
        let mut input_stream = self.input_config.create_input_stream();

        let (stop_sender, stop_receiver) = oneshot::channel::<()>();

        let handle_recorder = async move{
            select! {
                _ = async{
                    while let Some(data) = input_stream.next().await{
                        let mut buffer = buffer_input.lock().await;
                        // println!("get lock in recorder");
                        buffer.push_back(data);
                    }
                } => {},
                _ = stop_receiver => {
                    println!("stop recording");
                },
            }
        };

        let handle_detect_preamble =self.detect_preamble(time_limit);

        select! {
            _ = handle_recorder => {},
            result_detect = handle_detect_preamble => {
                match  result_detect {
                    Ok(align_result) => {
                        println!("detect preamble success, start receiving data");
                        let handle_recv_frame = self.recv_frame(&align_result).await;
                        match handle_recv_frame{
                            Ok(_) => {
                                println!("receive frame success");
                            }
                            Err(_) => {
                                println!("receive frame failed");
                            }
                        }
                        stop_sender.send(()).unwrap();
                        
                    }
                    Err(_) => {
                        println!("detect preamble failed");
                    }
                }
            }
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
    pub buffer: VecDeque<Vec<f32>>,
    demodulate_config: DemodulationConfig,
    writer: File,
}

impl Demodulation2{
    pub fn new(carrier_freq: Vec<u32>, sample_rate: u32, enable_ofdm: bool, output_file: &str) -> Self{
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
            let ref_sin = (0..ref_len).map(|t| (2.0 * std::f64::consts::PI * *carrier as f64 * (t as f64 / sample_rate as f64)).sin()).collect::<Vec<f64>>();
            for _ in 0..phy_frame::FRAME_PREAMBLE_LENGTH/2{
                preamble.extend(ref_sin.iter());
                preamble.extend(ref_sin.iter().map(|x| -*x));
            }
            ref_signal.push(ref_sin);

        }

        let demodulation_config = DemodulationConfig::new(carrier_freq, enable_ofdm, ref_signal, ref_signal_len, preamble); 

        let writer = File::create(output_file).unwrap();

        Demodulation2{
            input_config: input_stream_config,
            buffer: VecDeque::new(),
            demodulate_config: demodulation_config,
            writer,
        }
    }

    pub async fn listening(&mut self, write_to_file: bool, data: VecDeque<Vec<f32>>, debug_vec: &mut Vec<f32>) -> Vec<u8>{
        let mut input_stream = self.input_config.create_input_stream();
        let demodulate_config = &self.demodulate_config;
        let window_size = 10;

        // let mut debug_vec = Vec::new();

        let mut demodulate_state = DemodulationState::DetectPreamble;

        let mut avg_power = 0.0;
        let factor = 1.0 / 64.0;

        let mut tmp_buffer: VecDeque<f32> = VecDeque::new();
        let mut tmp_buffer_len = tmp_buffer.len();

        let mut local_max = 0.0;
        let mut start_index = usize::MAX;

        let mut result = Vec::new();
        let mut tmp_bits_data: Vec<u8> = Vec::new();

        while let Some(data) = input_stream.next().await{
        // for data in data{
            if demodulate_state == DemodulationState::Stop {
                break;
            }

            tmp_buffer_len += data.len();
            for i in data{
                avg_power = avg_power * (1.0 - factor) + i as f64 * factor;
                tmp_buffer.push_back(i);
            }
            
            if demodulate_state == DemodulationState::DetectPreamble{
                if tmp_buffer_len <= demodulate_config.preamble_len{
                    continue;
                }

                for i in 0..tmp_buffer_len-demodulate_config.preamble_len{
                    let window = tmp_buffer.range(i..i+demodulate_config.preamble_len);
                    // let dot_product = dot_product_smooth(window.clone().map(|x| *x).collect::<Vec<f32>>().as_slice(), 
                    //                                           demodulate_config.preamble.iter().map(|x| *x).collect::<Vec<f64>>().as_slice(), 
                    //                                           window_size);

                    let dot_product = dot_product(window.clone().map(|x| *x).collect::<Vec<f32>>().as_slice(), 
                                                  demodulate_config.preamble.iter().map(|x| *x).collect::<Vec<f64>>().as_slice());

                    
                    println!("dot_product: {:?}, local_max: {:?}", dot_product, local_max);
                    if dot_product > avg_power * 2.0 && dot_product > local_max && dot_product > 0.0{
                        local_max = dot_product;
                        start_index = i+1;
                        debug_vec.clear();
                        debug_vec.extend(smooth(window.clone().map(|x| *x).collect::<Vec<f32>>().as_slice(), window_size))
                        // debug_vec.extend(window.clone());
                    }
                    else if start_index != usize::MAX && i - start_index > demodulate_config.preamble_len/2 && local_max > 0.2{
                        println!("have detected preamble !!");
                        println!("local_max: {:?}", local_max);
                        demodulate_state = demodulate_state.next();

                        local_max = 0.0;
                        start_index += demodulate_config.preamble_len - 1;
                        tmp_bits_data.clear();
                        tmp_bits_data.extend(vec![0, 1, 0, 1, 0, 1, 0, 1, 0, 1]);
                        // tmp_bits_data.extend(vec![0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1]);
                        break;
                    }
                }
            }
            else if demodulate_state == DemodulationState::RecvFrame{
                if tmp_buffer_len <= demodulate_config.ref_signal_len[0]{
                    continue;
                }

                while tmp_buffer_len - start_index >= demodulate_config.ref_signal_len[0] && tmp_bits_data.len() < phy_frame::frame_length_length()+phy_frame::FRAME_PAYLOAD_LENGTH + phy_frame::FRAME_PREAMBLE_LENGTH{
                    let dot_product = dot_product_smooth(tmp_buffer.range(start_index..start_index+demodulate_config.ref_signal_len[0]).map(|x| *x).collect::<Vec<f32>>().as_slice(), 
                                                              demodulate_config.ref_signal[0].iter().map(|x| *x).collect::<Vec<f64>>().as_slice(), 
                                                              window_size);

                    // let dot_product = dot_product(tmp_buffer.range(start_index..start_index+demodulate_config.ref_signal_len[0]).map(|x| *x).collect::<Vec<f32>>().as_slice(), 
                    //                               demodulate_config.ref_signal[0].iter().map(|x| *x).collect::<Vec<f64>>().as_slice());

                    tmp_bits_data.push(if dot_product > 0.0 {0} else {1});

                    // debug_vec.extend(tmp_buffer.range(start_index..start_index+demodulate_config.ref_signal_len[0]));
                    debug_vec.extend(smooth(&tmp_buffer.range(start_index..start_index+demodulate_config.ref_signal_len[0]).map(|x| *x).collect::<Vec<f32>>()[..], window_size));
                    start_index += demodulate_config.ref_signal_len[0];
                }

                if tmp_bits_data.len() == phy_frame::frame_length_length()+phy_frame::FRAME_PAYLOAD_LENGTH + phy_frame::FRAME_PREAMBLE_LENGTH{
                    let mut loop_count = 0;
                    let mut ones_count = 0;
                    let mut data_len = 0;
                    for i in phy_frame::FRAME_PREAMBLE_LENGTH..phy_frame::frame_length_length()+phy_frame::FRAME_PREAMBLE_LENGTH{
                        ones_count += tmp_bits_data[i];
                        loop_count += 1;
                        if loop_count == 3{
                            data_len <<= 1;
                            if ones_count > 1{
                                data_len += 1;
                            }

                            ones_count = 0;
                            loop_count = 0;
                        }
                    }

                    let length = utils::read_data_2_compressed_u8(tmp_bits_data.iter().take(phy_frame::frame_length_length()+phy_frame::FRAME_PREAMBLE_LENGTH).cloned().collect());
                    println!("length: {:?}", length);
                    println!("actual len: {:?}", data_len);

                    let mut recv_data= utils::read_data_2_compressed_u8(tmp_bits_data.iter().skip(phy_frame::frame_length_length()+phy_frame::FRAME_PREAMBLE_LENGTH).cloned().collect());
                    println!("recv_data: {:?}", recv_data);

                    // construct the payload (to fit in the shard macro)
                    let mut i = 0;
                    let mut payload = phy_frame::PHYFrame::construct_payload_format(recv_data);

                    result.extend(&utils::read_compressed_u8_2_data(phy_frame::PHYFrame::payload_2_data(payload).unwrap())[..data_len]);
                    
                    if data_len == phy_frame::MAX_FRAME_DATA_LENGTH{
                        demodulate_state = demodulate_state.return_detect_preamble();
                        println!("return to detect preamble");
                    }
                    else{
                        demodulate_state = demodulate_state.next();
                        println!("stop receiving data");
                    }
                    // demodulate_state = DemodulationState::Stop;
                }
            }

            if start_index == usize::MAX{
                for i in 0..tmp_buffer_len - demodulate_config.preamble_len+1{
                    tmp_buffer.pop_front();
                }
            }
            else{
                for i in 0..start_index{
                    tmp_buffer.pop_front();
                }
                start_index = 0;
            }
            tmp_buffer_len = tmp_buffer.len();
        }

        if write_to_file{
            self.writer.write_all(result.clone().iter().map(|x| x + b'0').collect::<Vec<u8>>().as_slice()).unwrap();
        }

        result
        // debug_vec
    }
}