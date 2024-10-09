use crate::acoustic_modem::phy_frame::{self, PHYFrame};
use crate::asio_stream::InputAudioStream;
use crate::utils::{
    read_compressed_u8_2_data, read_data_2_compressed_u8, u8_2_code_rs_hexbit, Bit, Byte,
};
use anyhow::Error;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, SampleRate, SupportedStreamConfig};
use futures::StreamExt;
use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;
use std::ops::{Add, Mul};

struct InputStreamConfig {
    config: SupportedStreamConfig,
    device: Device,
}

impl InputStreamConfig {
    fn new(config: SupportedStreamConfig, device: Device) -> Self {
        InputStreamConfig { config, device }
    }

    fn create_input_stream(&self) -> InputAudioStream {
        // println!("create input stream");
        // println!("config: {:?}", self.config);
        // println!("device: {:?}", self.device.name());

        InputAudioStream::new(&self.device, self.config.clone())
    }
}

struct DemodulationConfig {
    carrier_freq: Vec<u32>,
    sample_rate: u32,
    ref_signal: Vec<Vec<f32>>,
    ref_signal_len: usize,
    preamble_len: usize,
    preamble: Vec<f32>,
}

unsafe impl Send for DemodulationConfig {}
unsafe impl Sync for DemodulationConfig {}

impl DemodulationConfig {
    fn new(
        carrier_freq: Vec<u32>,
        sample_rate: u32,
        ref_signal: Vec<Vec<f32>>,
        ref_signal_len: usize,
    ) -> Self {
        let preamble = phy_frame::gen_preamble(sample_rate);
        // println!("preamble len: {}", preamble.len());
        DemodulationConfig {
            carrier_freq,
            sample_rate,
            ref_signal,
            ref_signal_len,
            preamble_len: preamble.len(),
            preamble,
        }
    }
}

#[derive(PartialEq, Debug)]
enum DemodulationState {
    DetectPreamble,
    RecvFrame,
    Stop,
}

impl DemodulationState {
    pub fn next(&self) -> Self {
        // println!("switched");

        match self {
            DemodulationState::DetectPreamble => DemodulationState::RecvFrame,
            DemodulationState::RecvFrame => DemodulationState::DetectPreamble,
            DemodulationState::Stop => DemodulationState::Stop,
        }
    }
}

pub fn dot_product(input: &[f32], ref_signal: &[f32]) -> f32 {
    if input.len() != ref_signal.len() {
        panic!("Input length is not equal to reference signal length");
    }

    dot_product_iter(input.iter(), ref_signal.iter())
}

pub fn dot_product_iter<I, J, T, U, V>(iter1: I, iter2: J) -> V
where
    I: Iterator<Item = T>,
    J: Iterator<Item = U>,
    T: Mul<U, Output = V>,
    V: Add<Output = V> + Default,
{
    iter1
        .zip(iter2)
        .map(|(a, b)| a * b)
        .fold(V::default(), |acc, x| acc + x)
}

pub struct Demodulation2 {
    input_config: InputStreamConfig,
    demodulate_config: DemodulationConfig,
    writer: File,
}

impl Demodulation2 {
    pub fn new(
        carrier_config: Vec<u32>,
        sample_rate: u32,
        output_file: &str,
        redundent_times: usize,
    ) -> Self {
        // let host = cpal::host_from_id(cpal::HostId::Asio).expect("failed to initialise ASIO host");
        let host = cpal::default_host();
        let device = host.input_devices().expect("failed to find input device");
        let device = device
            .into_iter()
            .next()
            .expect("no input device available");
        println!("Input device: {:?}", device.name().unwrap());

        let default_config = device.default_input_config().unwrap();
        let config = SupportedStreamConfig::new(
            default_config.channels(),
            // 1,                   // mono
            SampleRate(sample_rate), // sample rate
            default_config.buffer_size().clone(),
            default_config.sample_format(),
        );

        println!("config: {:?}", config);

        let input_stream_config = InputStreamConfig::new(config, device);

        // sort carrier_freq in ascending order
        let mut carrier_freq = vec![];
        for i in 0..carrier_config[2]{
            carrier_freq.push(carrier_config[0] + i * carrier_config[1]);
        }
        // carrier_freq.push(6000);
        println!("carrier freq: {:?}", carrier_freq);

        let mut ref_signal = Vec::new();
        let ref_len = (sample_rate / carrier_freq[0]) as usize * redundent_times;
        println!("ref len: {}", ref_len);

        for i in 0..carrier_freq.len() {
            let carrier = carrier_freq.get(i).unwrap();
            let ref_sin = (0..ref_len)
                .map(|t| {
                    (2.0 * std::f32::consts::PI * *carrier as f32 * (t as f32 / sample_rate as f32))
                        .sin()
                })
                .collect::<Vec<f32>>();
            ref_signal.push(ref_sin);

            // let single_len = sample_rate / carrier_freq[i];
            // let ref_line: Vec<f32> = (0..single_len).map(|item| 1.0 - 2.0 * (item as f32) / single_len as f32).collect();
            // let mut ref_total = Vec::with_capacity(ref_len);
            // for j in 0..ref_len{
            //     let ref_index = j % single_len as usize;
            //     ref_total.push(ref_line[ref_index]);
            // }
            // ref_signal.push(ref_total);
        }

        let demodulation_config =
            DemodulationConfig::new(carrier_freq, sample_rate, ref_signal, ref_len);

        let writer = File::create(output_file).unwrap();

        Demodulation2 {
            input_config: input_stream_config,
            demodulate_config: demodulation_config,
            writer,
        }
    }

    pub async fn simple_listen(
        &mut self,
        write_to_file: bool,
        debug_vec: &mut Vec<f32>,
        data_len: usize,
        padding_len: usize,
    ) -> Vec<u8> {
        let data_len = data_len;

        let mut input_stream = self.input_config.create_input_stream();
        let demodulate_config = &self.demodulate_config;
        let alpha_check = 0.31;
        let mut prev = 0.0;

        let mut demodulate_state = DemodulationState::DetectPreamble;

        let mut avg_power = 0.0;
        let power_lim_preamble = 0.5;
        let factor = 1.0 / 64.0;

        let mut tmp_buffer: VecDeque<f32> = VecDeque::with_capacity(
            15 * demodulate_config
                .preamble_len
                .max(demodulate_config.ref_signal_len),
        );
        let mut tmp_buffer_len = tmp_buffer.len();

        let mut local_max = 0.0;
        let mut start_index = usize::MAX;

        let mut tmp_bits_data = Vec::with_capacity(data_len);
        let channels = self.input_config.config.channels() as usize;
        // let mut res = vec![];

        while let Some(data) = input_stream.next().await {
            if demodulate_state == DemodulationState::Stop {
                break;
            }

            // debug_vec.extend(data.clone().iter());
            tmp_buffer_len += data.len() / channels;
            // let data = data.iter().map(|&sample| filter.run(sample)).collect();
            move_data_into_buffer(data, &mut tmp_buffer, alpha_check, channels, &mut prev);

            if demodulate_state == DemodulationState::DetectPreamble {
                if tmp_buffer_len <= demodulate_config.preamble_len + padding_len {
                    continue;
                }
                tmp_buffer.make_contiguous();
                for i in 0..tmp_buffer_len - self.demodulate_config.preamble_len - 1 - padding_len {
                    // let window = tmp_buffer.range(i..i+demodulate_config.preamble_len);
                    let window = &tmp_buffer.as_slices().0[i..i + demodulate_config.preamble_len];
                    let dot_product = dot_product(window, &demodulate_config.preamble);

                    if dot_product > avg_power * 2.0
                        && dot_product > local_max
                        && dot_product > power_lim_preamble
                    {
                        // println!("detected");
                        local_max = dot_product;
                        start_index = i + 1;
                        debug_vec.clear();
                        debug_vec.extend(window);
                    } else if start_index != usize::MAX
                        && i - start_index > demodulate_config.preamble_len
                        && local_max > power_lim_preamble
                    {
                        local_max = 0.0;
                        start_index += demodulate_config.preamble_len - 1 + padding_len;
                        demodulate_state = demodulate_state.next();
                        println!("detected preamble");
                        break;
                    }
                }
            }

            if demodulate_state == DemodulationState::RecvFrame {
                if tmp_buffer_len - start_index <= demodulate_config.ref_signal_len {
                    continue;
                }
                tmp_buffer.make_contiguous();

                while tmp_buffer_len - start_index >= demodulate_config.ref_signal_len
                    && tmp_bits_data.len() < data_len
                {
                    // let dot_product = range_dot_product_vec(tmp_buffer.range(start_index..start_index+demodulate_config.ref_signal_len), &self.demodulate_config.ref_signal[0]);
                    let dot_product = dot_product(
                        &tmp_buffer.as_slices().0
                            [start_index..start_index + demodulate_config.ref_signal_len],
                        &self.demodulate_config.ref_signal[0],
                    );
                    debug_vec.extend(
                        tmp_buffer
                            .range(start_index..start_index + demodulate_config.ref_signal_len),
                    );

                    start_index += demodulate_config.ref_signal_len;

                    tmp_bits_data.push(if dot_product >= 0.0 { 0 } else { 1 });
                }
                // println!("current recv length: {}", tmp_bits_data.len());
            }

            if tmp_bits_data.len() >= data_len {
                demodulate_state = DemodulationState::Stop;
            }

            let pop_times = if start_index == usize::MAX {
                tmp_buffer_len - demodulate_config.preamble_len + 1
            } else {
                start_index
            };

            for i in 0..pop_times {
                tmp_buffer.pop_front();
            }

            start_index = if start_index == usize::MAX {
                usize::MAX
            } else {
                0
            };
            tmp_buffer_len = tmp_buffer.len();
            // println!("tmp buffer len: {}", tmp_buffer_len);
        }

        if write_to_file {
            self.writer
                .write_all(
                    &tmp_bits_data
                        .clone()
                        .iter()
                        .map(|x| x + b'0')
                        .collect::<Vec<u8>>(),
                )
                .unwrap()
        }
        // println!("recv data: {:?}", tmp_bits_data);
        // println!("data: {:?}", tmp_bits_data);

        tmp_bits_data
    }

    pub async fn listening(
        &mut self,
        write_to_file: bool,
        data_len: usize,
        decoded_data: &mut Vec<u8>,
        debug_vec: &mut Vec<f32>,
        test_data: Vec<Vec<f32>>
    ) {
        // let data_len = data_len;

        let mut input_stream = self.input_config.create_input_stream();
        let demodulate_config = &self.demodulate_config;
        let alpha_check = 0.31;
        let mut prev = 0.0;

        let mut demodulate_state = DemodulationState::DetectPreamble;

        let power_lim_preamble = 5.0;

        let mut tmp_buffer: VecDeque<f32> = VecDeque::with_capacity(
            5 * demodulate_config
                .preamble_len
                .max(demodulate_config.ref_signal_len),
        );
        let mut tmp_buffer_len = tmp_buffer.len();

        let mut local_max = 0.0;
        let mut start_index = usize::MAX;

        let carrier_num = demodulate_config.carrier_freq.len();
        // let mut tmp_bits_data = vec![vec![0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 1, 0, 1, 1, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 1, 1, 0, 0, 1, 1, 1, 0, 0, 1, 0, 1, 0, 1, 1, 0, 1, 1, 1, 0, 0, 1, 1, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 1, 1], vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 1, 0, 0, 1, 1, 1, 1, 1, 1, 1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 0, 1, 1, 1, 1, 0, 1, 0, 1, 1, 1, 1, 1, 0, 0, 1, 1, 0, 1, 1, 1]];
        let mut tmp_bits_data = vec![Vec::with_capacity(data_len); carrier_num];
        let mut is_reboot = false;

        let channels = self.input_config.config.channels() as usize;

        while let Some(data) = input_stream.next().await {
        // for data in test_data{
            if demodulate_state == DemodulationState::Stop {
                break;
            }
            tmp_buffer_len += data.len() / channels;
            move_data_into_buffer(data, &mut tmp_buffer, alpha_check, channels, &mut prev);
            // println!("buffer len: {}", tmp_buffer_len);
            // tmp_buffer.extend(data.iter());

            if demodulate_state == DemodulationState::DetectPreamble {
                if tmp_buffer_len <= demodulate_config.preamble_len {
                    // println!("buffer is smaller than preamble");
                    continue;
                }
                // println!("start detect preamble");
                // println!("for end: {}", tmp_buffer_len - self.demodulate_config.preamble_len-1);
                tmp_buffer.make_contiguous();
                for i in 0..tmp_buffer_len - demodulate_config.preamble_len - 1 {
                    // let window = tmp_buffer.range(i..i+demodulate_config.preamble_len);
                    let window = &tmp_buffer.as_slices().0[i..i + demodulate_config.preamble_len];
                    let dot_product = dot_product(window, &demodulate_config.preamble);

                    if dot_product > local_max && dot_product > power_lim_preamble {
                        local_max = dot_product;
                        // println!("detected, local max: {}", local_max);
                        start_index = i + 1;
                        debug_vec.clear();
                        debug_vec.extend(window);
                    } else if start_index != usize::MAX
                        && i - start_index > demodulate_config.preamble_len
                        && local_max > power_lim_preamble
                    {
                        local_max = 0.0;
                        start_index += demodulate_config.preamble_len - 1;
                        demodulate_state = demodulate_state.next();
                        // println!("detected preamble");
                        // println!("start index: {}, tmp buffer len: {}", start_index, tmp_buffer_len);
                        break;
                    }
                }
                // println!("start index: {}", start_index);
            }

            if demodulate_state == DemodulationState::RecvFrame {
                if tmp_buffer_len < start_index
                    || tmp_buffer_len - start_index <= demodulate_config.ref_signal_len
                {
                    // println!("tmp buffer is not long enough");
                    continue;
                }
                tmp_buffer.make_contiguous();

                while tmp_buffer_len - start_index >= demodulate_config.ref_signal_len
                    && tmp_bits_data[0].len() < data_len
                {
                    let window = &tmp_buffer.as_slices().0
                                [start_index..start_index + demodulate_config.ref_signal_len];
                    for i in 0..carrier_num{
                        let dot_product = dot_product(
                            window,
                            &self.demodulate_config.ref_signal[i],
                        );
                        tmp_bits_data[i].push(if dot_product >= 0.0 { 0 } else { 1 });
                    }
                    debug_vec.extend(
                        window,
                    );
                    start_index += demodulate_config.ref_signal_len;
                    
                }
            }

            if tmp_bits_data[0].len() >= data_len {
                // demodulate_state = demodulate_state.return_detect_preamble();
                is_reboot = true;
                demodulate_state = demodulate_state.next();
                // demodulate_state = DemodulationState::Stop;

                for i in 0..carrier_num{                    
                    let result = decode(tmp_bits_data[i].clone());
                    tmp_bits_data[i].clear();
                    match result {
                        Ok((vec, length)) => {
                            println!("length: {}", length);
                            if length > phy_frame::MAX_FRAME_DATA_LENGTH {
                                println!("freq {}, wrong data length", demodulate_config.carrier_freq[i]);
                            } else {
                                let decompressed = read_compressed_u8_2_data(vec)[0..length].to_vec();

                                if write_to_file {
                                    let to_write = &decompressed
                                        .clone()
                                        .iter()
                                        .map(|x| *x + b'0')
                                        .collect::<Vec<u8>>();
                                    // println!("to write: {:?}", to_write);
                                    self.writer.write_all(to_write).unwrap();
                                }
                                decoded_data.extend_from_slice(&decompressed[0..length]);
                                println!("freq {}, received", demodulate_config.carrier_freq[i]);
                            }
                        }

                        Err(_) => {
                            println!("Error: received invalid data");
                        }
                    };
                }
            }

            let pop_times = if start_index == usize::MAX {
                tmp_buffer_len - demodulate_config.preamble_len + 1
            } else {
                start_index
            };

            for _ in 0..pop_times {
                tmp_buffer.pop_front();
            }

            start_index = if start_index == usize::MAX || is_reboot {
                is_reboot = false;
                usize::MAX
            } else {
                0
            };
            tmp_buffer_len = tmp_buffer.len();
            // println!("tmp bit len: {:?}", tmp_bits_data[0].len());
            // println!("buffer len: {}", tmp_buffer_len);
        }
    }
}

fn decode(input_data: Vec<Bit>) -> Result<(Vec<Byte>, usize), Error> {
    // println!(
    //     "input data: {:?}, data length: {}",
    //     input_data,
    //     input_data.len()
    // );
    let compressed = read_data_2_compressed_u8(input_data.clone());
    let hexbits = u8_2_code_rs_hexbit(read_data_2_compressed_u8(input_data));

    // println!("hexbits: {:?}, hexbit length: {}", hexbits, hexbits.len());
    let decoded = PHYFrame::payload_2_data(hexbits.clone());

    let decoded_data = decoded.unwrap().0;

    let mut correct_num = 0;
    for i in 0..decoded_data.len(){
        if compressed[i] != decoded_data[i]{
            correct_num += 1;
        }
    }

    println!("correct num: {}", correct_num);

    PHYFrame::payload_2_data(hexbits)    
}

fn move_data_into_buffer(
    data: Vec<f32>,
    buffer: &mut VecDeque<f32>,
    smooth_alpha: f32,
    channels: usize,
    prev: &mut f32,
) {
    for (index, &i) in data.iter().enumerate() {
        if index % channels == 0 {
            let processed_signal = i * smooth_alpha + *prev * (1.0 - smooth_alpha);
            *prev = i;
            buffer.push_back(processed_signal);
        }
    }
}
