use crate::acoustic_modem::modulation::{self, ENABLE_ECC};
use crate::acoustic_modem::phy_frame::{self, PHYFrame};
use crate::asio_stream::InputAudioStream;
use crate::utils::{read_data_2_compressed_u8, u8_2_code_rs_hexbit, Bit, Byte};
use anyhow::Error;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, SampleRate, SupportedStreamConfig};
use futures::StreamExt;
use std::collections::VecDeque;
use std::ops::{Add, Mul};
use std::result::Result::Ok;
use std::{mem, vec};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

pub enum SwitchSignal {
    STOP_SIGNAL,
    RESUME_SIGNAL,
    SWITCH_SIGNAL,
}

// pub const SWITCH_SIGNAL: SwitchSignal = 2;
// pub const STOP_SIGNAL: SwitchSignal = 0;
// pub const RESUME_SIGNAL: SwitchSignal = 1;
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
    ref_signal: Vec<Vec<f32>>,
    ref_signal_len: Vec<usize>,
    preamble_len: usize,
    preamble: Vec<f32>,
    payload_bits_length: usize,
    data_bits_length: usize,
    lowest_power_limit: f32,
}

unsafe impl Send for DemodulationConfig {}
unsafe impl Sync for DemodulationConfig {}

impl DemodulationConfig {
    fn new(
        sample_rate: u32,
        ref_signal: Vec<Vec<f32>>,
        ref_signal_len: Vec<usize>,
        payload_bits_length: usize,
        data_bits_length: usize,
        lowest_power_limit: f32,
    ) -> Self {
        let preamble = phy_frame::gen_preamble(sample_rate);
        // println!("preamble len: {}", preamble.len());
        DemodulationConfig {
            ref_signal,
            ref_signal_len,
            preamble_len: preamble.len(),
            preamble,
            payload_bits_length,
            data_bits_length,
            lowest_power_limit,
        }
    }
}

#[derive(PartialEq, Debug)]
pub enum DemodulationState {
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

    pub fn stop(&self) -> Self {
        DemodulationState::Stop
    }

    pub fn resume(&self) -> Self {
        DemodulationState::DetectPreamble
    }

    pub fn switch(&mut self) {
        if *self == DemodulationState::Stop {
            *self = DemodulationState::DetectPreamble;
        } else {
            *self = DemodulationState::Stop;
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
    // pub buffer: VecDeque<Vec<f32>>,
    demodulate_config: DemodulationConfig,
}

unsafe impl Send for Demodulation2 {}

impl Demodulation2 {
    pub fn new(
        carrier_freq: Vec<u32>,
        sample_rate: u32,
        redundent_times: usize,
        enable_ofdm: bool,
        payload_bits_length: usize,
        data_bits_length: usize,
        lowest_power_limit: f32,
    ) -> Self {
        let host = cpal::host_from_id(cpal::HostId::Asio).expect("failed to initialise ASIO host");
        // let host = cpal::default_host();
        let device = host.input_devices().expect("failed to find input device");
        let device = device
            .into_iter()
            .next()
            .expect("no input device available");
        println!("Input device: {:?}", device.name().unwrap());

        let default_config = device.default_input_config().unwrap();
        let config = SupportedStreamConfig::new(
            // default_config.channels(),
            1,                       // mono
            SampleRate(sample_rate), // sample rate
            default_config.buffer_size().clone(),
            default_config.sample_format(),
        );

        println!("config: {:?}", config);

        let input_stream_config = InputStreamConfig::new(config, device);

        // sort carrier_freq in ascending order
        let mut carrier_freq = carrier_freq;
        carrier_freq.sort();

        let mut ref_signal = Vec::new();
        let mut ref_signal_len = Vec::new();
        let ref_len = (sample_rate / carrier_freq[0]) as usize * redundent_times;

        for i in 0..carrier_freq.len() {
            let carrier = carrier_freq.get(i).unwrap();
            ref_signal_len.push(ref_len);
            let ref_sin = (0..ref_len)
                .map(|t| {
                    (2.0 * std::f32::consts::PI * *carrier as f32 / sample_rate as f32 * t as f32)
                        .sin()
                })
                .collect::<Vec<f32>>();
            ref_signal.push(ref_sin);
        }
        if !enable_ofdm {
            ref_signal = vec![ref_signal[0].clone()];
            ref_signal_len = vec![ref_signal_len[0].clone()];
        }

        let demodulation_config = DemodulationConfig::new(
            sample_rate,
            ref_signal,
            ref_signal_len,
            payload_bits_length,
            data_bits_length,
            lowest_power_limit,
        );

        Demodulation2 {
            input_config: input_stream_config,
            // buffer: VecDeque::new(),
            demodulate_config: demodulation_config,
        }
    }

    pub async fn listening(&mut self, decoded_data: &mut Vec<u8>) {
        let demodulate_config = &self.demodulate_config;
        let payload_len = demodulate_config.payload_bits_length;
        // let bits_len = demodulate_config.data_bits_length;
        // let bits_len = if ENABLE_ECC {
        //     phy_frame::MAX_FRAME_DATA_LENGTH
        // } else {
        //     phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING
        // };

        let mut input_stream = self.input_config.create_input_stream();
        let alpha_check = 1.0;
        let mut prev = 0.0;

        let mut demodulate_state = DemodulationState::DetectPreamble;

        let power_lim_preamble = demodulate_config.lowest_power_limit;

        let mut tmp_buffer: VecDeque<f32> = VecDeque::with_capacity(
            5 * demodulate_config
                .preamble_len
                .max(demodulate_config.ref_signal_len[0]),
        );
        let mut tmp_buffer_len = tmp_buffer.len();

        let mut local_max = 0.0;
        let mut start_index = usize::MAX;

        let carrier_num = demodulate_config.ref_signal.len();
        // println!("low bound {}", power_lim_preamble);
        let mut tmp_bits_data = vec![Vec::with_capacity(payload_len); carrier_num];

        let mut is_reboot = false;

        let channels = self.input_config.config.channels() as usize;

        let mut last_frame_index = 0;
        let mut debug_vec: Vec<f32> = vec![];
        while let Some(data) = input_stream.next().await {
            if demodulate_state == DemodulationState::Stop {
                continue;
            }
            // println!("data len: {}", data.len());
            tmp_buffer_len += data.len() / channels;
            move_data_into_buffer(data, &mut tmp_buffer, alpha_check, channels, &mut prev);
            if demodulate_state == DemodulationState::DetectPreamble {
                if tmp_buffer_len <= demodulate_config.preamble_len {
                    continue;
                }
                tmp_buffer.make_contiguous();
                for i in 0..tmp_buffer_len - demodulate_config.preamble_len - 1 {
                    let window = &tmp_buffer.as_slices().0[i..i + demodulate_config.preamble_len];
                    let dot_product = dot_product(window, &demodulate_config.preamble);
                    // println!("product: {:?}", dot_product);
                    if dot_product > local_max && dot_product > power_lim_preamble {
                        // println!("detected");
                        local_max = dot_product;
                        start_index = i + 1;
                        // debug_vec.clear();
                        // debug_vec.extend(window);
                    } else if start_index != usize::MAX
                        && i - start_index > demodulate_config.preamble_len
                        && local_max > power_lim_preamble
                    {
                        if ((last_frame_index + start_index) as isize
                            - modulation::OFDM_FRAME_DISTANCE as isize)
                            .abs()
                            > 10
                        {
                            println!("last frame distance: {}", last_frame_index + start_index);
                        }
                        start_index += demodulate_config.preamble_len - 1;
                        demodulate_state = demodulate_state.next();
                        // println!("detected preamble");
                        println!(
                            "start index: {}, tmp buffer len: {}, max: {}",
                            start_index, tmp_buffer_len, local_max
                        );
                        local_max = 0.0;
                        // println!("debug vec: {:?}", debug_vec);
                        break;
                    }
                }
                // println!("start index: {}", start_index);
            }

            if demodulate_state == DemodulationState::RecvFrame {
                if tmp_buffer_len < start_index
                    || tmp_buffer_len - start_index <= demodulate_config.ref_signal_len[0]
                {
                    // println!("tmp buffer is not long enough");
                    continue;
                }
                tmp_buffer.make_contiguous();
                // println!("start index: {}, tmp_buffer_len: {}", start_index, tmp_buffer_len);

                while tmp_buffer_len - start_index >= demodulate_config.ref_signal_len[0]
                    && tmp_bits_data[0].len() < payload_len
                {
                    let window = &tmp_buffer.as_slices().0
                        [start_index..start_index + demodulate_config.ref_signal_len[0]];
                    debug_vec.extend_from_slice(window);
                    for k in 0..carrier_num {
                        let dot_product =
                            dot_product(window, &self.demodulate_config.ref_signal[k]);
                        // println!("product: {}", dot_product);
                        // debug_vec.extend(
                        //     &tmp_buffer.as_slices().0
                        //         [start_index..start_index + demodulate_config.ref_signal_len[k]],
                        // );

                        tmp_bits_data[k].push(if dot_product >= 0.0 { 0 } else { 1 });
                    }
                    start_index += demodulate_config.ref_signal_len[0];
                }
            }

            if tmp_bits_data[0].len() >= payload_len {
                // println!("debug vec: {:?}", debug_vec);
                is_reboot = true;
                demodulate_state = demodulate_state.next();
                last_frame_index = 0;
                for k in 0..carrier_num {
                    // println!("data: {:?}", tmp_bits_data[k]);
                    let result = decode(mem::replace(
                        &mut tmp_bits_data[k],
                        Vec::with_capacity(payload_len),
                    ));
                    // tmp_bits_data[k].clear();

                    match result {
                        Ok((meta_data, _)) => {
                            decoded_data.extend(meta_data.iter());
                        }
                        Err(msg) => {
                            println!("{}", msg);
                            // break;
                        }
                    }
                }
                // println!("tmp bit len: {}", tmp_bits_data.len());
            }

            let pop_times = if start_index == usize::MAX {
                tmp_buffer_len - demodulate_config.preamble_len + 1
            } else {
                start_index
            };
            if !is_reboot {
                last_frame_index += pop_times;
            }
            for _ in 0..pop_times {
                tmp_buffer.pop_front();
            }

            start_index = if start_index == usize::MAX || is_reboot {
                is_reboot = false;
                // println!("reboot");
                usize::MAX
            } else {
                0
            };
            tmp_buffer_len = tmp_buffer.len();

            // println!("buffer len: {}", tmp_buffer_len);
        }

        println!("listen stoped");
    }

    pub async fn listening_daemon(
        &mut self,
        output_tx: UnboundedSender<Vec<Byte>>,
        mut state_rx: UnboundedReceiver<SwitchSignal>,
        init_state: DemodulationState,
    ) -> Result<(), anyhow::Error> {
        let demodulate_config = &self.demodulate_config;
        let payload_len = demodulate_config.payload_bits_length;

        let mut input_stream = self.input_config.create_input_stream();
        let alpha_check = 1.0;
        let mut prev = 0.0;

        let mut demodulate_state = init_state;

        let power_lim_preamble = demodulate_config.lowest_power_limit;

        let mut tmp_buffer: VecDeque<f32> = VecDeque::with_capacity(
            5 * demodulate_config
                .preamble_len
                .max(demodulate_config.ref_signal_len[0]),
        );
        let mut tmp_buffer_len = tmp_buffer.len();

        let mut local_max = 0.0;
        let mut start_index = usize::MAX;

        let carrier_num = demodulate_config.ref_signal.len();
        // println!("low bound {}", power_lim_preamble);
        let mut tmp_bits_data = vec![Vec::with_capacity(payload_len); carrier_num];

        let mut is_reboot = false;

        let channels = self.input_config.config.channels() as usize;

        let mut last_frame_index = 0;

        println!("listen daemon start");
        while let Some(data) = input_stream.next().await {
            if let Ok(signal) = state_rx.try_recv() {
                match signal {
                    SwitchSignal::STOP_SIGNAL => {
                        demodulate_state = demodulate_state.stop();
                    }
                    SwitchSignal::RESUME_SIGNAL => {
                        demodulate_state = demodulate_state.resume();
                    }
                    SwitchSignal::SWITCH_SIGNAL => {
                        demodulate_state.switch();
                    }
                }
            }

            if demodulate_state == DemodulationState::Stop {
                // println!("stoped");
                continue;
            }
            // println!("data len: {}", data.len());
            tmp_buffer_len += data.len() / channels;
            move_data_into_buffer(data, &mut tmp_buffer, alpha_check, channels, &mut prev);
            if demodulate_state == DemodulationState::DetectPreamble {
                if tmp_buffer_len <= demodulate_config.preamble_len {
                    continue;
                }
                tmp_buffer.make_contiguous();
                for i in 0..tmp_buffer_len - demodulate_config.preamble_len - 1 {
                    let window = &tmp_buffer.as_slices().0[i..i + demodulate_config.preamble_len];
                    let dot_product = dot_product(window, &demodulate_config.preamble);
                    if dot_product > local_max && dot_product > power_lim_preamble {
                        // println!("detected");
                        local_max = dot_product;
                        // println!("local max: {}", local_max);
                        start_index = i + 1;
                        // debug_vec.clear();
                        // debug_vec.extend(window);
                    } else if start_index != usize::MAX
                        && i - start_index > demodulate_config.preamble_len
                        && local_max > power_lim_preamble
                    {
                        if ((last_frame_index + start_index) as isize
                            - modulation::OFDM_FRAME_DISTANCE as isize)
                            .abs()
                            > 10
                        {
                            println!("last frame distance: {}", last_frame_index + start_index);
                        }
                        start_index += demodulate_config.preamble_len - 1;
                        demodulate_state = demodulate_state.next();
                        // println!("detected preamble");
                        println!(
                            "start index: {}, tmp buffer len: {}, max: {}",
                            start_index, tmp_buffer_len, local_max
                        );
                        local_max = 0.0;
                        break;
                    }
                }
                // println!("start index: {}", start_index);
            }

            if demodulate_state == DemodulationState::RecvFrame {
                if tmp_buffer_len < start_index
                    || tmp_buffer_len - start_index <= demodulate_config.ref_signal_len[0]
                {
                    // println!("tmp buffer is not long enough");
                    continue;
                }
                tmp_buffer.make_contiguous();
                // println!("start index: {}, tmp_buffer_len: {}", start_index, tmp_buffer_len);

                while tmp_buffer_len - start_index >= demodulate_config.ref_signal_len[0]
                    && tmp_bits_data[0].len() < payload_len
                {
                    let window = &tmp_buffer.as_slices().0
                        [start_index..start_index + demodulate_config.ref_signal_len[0]];
                    for k in 0..carrier_num {
                        let dot_product =
                            dot_product(window, &self.demodulate_config.ref_signal[k]);
                        // debug_vec.extend(
                        //     &tmp_buffer.as_slices().0
                        //         [start_index..start_index + demodulate_config.ref_signal_len[k]],
                        // );

                        tmp_bits_data[k].push(if dot_product >= 0.0 { 0 } else { 1 });
                    }
                    start_index += demodulate_config.ref_signal_len[0];
                }
            }

            if tmp_bits_data[0].len() >= payload_len {
                is_reboot = true;
                demodulate_state = demodulate_state.next();
                last_frame_index = 0;
                let mut to_send: Vec<Byte> = vec![];
                for k in 0..carrier_num {
                    // println!("data: {:?}", tmp_bits_data[k]);
                    let result = decode(mem::replace(
                        &mut tmp_bits_data[k],
                        Vec::with_capacity(payload_len),
                    ));
                    // tmp_bits_data[k].clear();

                    match result {
                        Ok((meta_data, _)) => {
                            to_send.extend(meta_data.iter());
                        }
                        Err(msg) => {
                            println!("{}", msg);
                            to_send.clear();
                            break;
                        }
                    }
                }
                if to_send.len() > 0 {
                    let _ = output_tx.send(to_send);
                }
                // println!("tmp bit len: {}", tmp_bits_data.len());
                // println!("reboot");
            }

            let pop_times = if start_index == usize::MAX {
                tmp_buffer_len - demodulate_config.preamble_len + 1
            } else {
                start_index
            };
            if !is_reboot {
                last_frame_index += pop_times;
            }
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

            // println!("channel size: {}", output_tx.strong_count());
            // println!("current state: {:?}", demodulate_state);
            // println!("buffer len: {}", tmp_buffer_len);
        }

        // println!("listen stoped");
        Ok(())
    }
}

fn decode(input_data: Vec<Bit>) -> Result<(Vec<Byte>, usize), Error> {
    // println!("input data: {:?}", input_data);
    if ENABLE_ECC {
        let hexbits = u8_2_code_rs_hexbit(read_data_2_compressed_u8(input_data));
        PHYFrame::payload_2_data(hexbits)
    } else {
        let compressed_data = read_data_2_compressed_u8(input_data);
        if !PHYFrame::check_crc(&compressed_data) {
            println!("wrong data: {:?}", compressed_data);
            return Err(Error::msg("CRC wrong"));
        }

        let mut length = 0;
        for i in 0..phy_frame::FRAME_LENGTH_LENGTH_NO_ENCODING / 8 {
            length <<= 8;
            length += compressed_data[i] as usize;
        }
        if length
            > if ENABLE_ECC {
                phy_frame::MAX_FRAME_DATA_LENGTH
            } else {
                phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING
            }
        {
            return Err(Error::msg("Length wrong"));
        }

        Ok((
            compressed_data[phy_frame::FRAME_LENGTH_LENGTH_NO_ENCODING / 8
                ..(phy_frame::FRAME_LENGTH_LENGTH_NO_ENCODING + length) / 8]
                .to_vec(),
            length,
        ))
    }
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
