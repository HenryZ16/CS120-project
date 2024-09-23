use std::collections::VecDeque;

/*
Input data
-> Modulation
-> Output Signal
*/
use super::phy_frame;
use crate::asio_stream::{AudioTrack, OutputAudioStream};
use crate::utils;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{HostId, SampleRate, SupportedStreamConfig};
use futures::SinkExt;
use hound::{WavSpec, WavWriter};
use rand_distr::Standard;

const SAMPLE_RATE: u32 = 48000;
const REDUNDANT_PERIODS: u32 = 4;

pub struct Modulator {
    carrier_freq: Vec<u32>,
    sample_rate: u32,
    redundant_periods: u32,
    enable_ofdm: bool,
    output_stream: OutputAudioStream<std::vec::IntoIter<f32>>,
    config: SupportedStreamConfig,
    device: cpal::Device,
}

impl Modulator {
    pub fn new(carrier_freq: Vec<u32>, sample_rate: u32, enable_ofdm: bool) -> Self {
        // let host = cpal::host_from_id(HostId::Asio).expect("failed to initialise ASIO host");
        let host = cpal::default_host();
        let device = host.output_devices().expect("failed to find output device");
        let device = device
            .into_iter()
            .next()
            .expect("no output device available");
        let device = host.default_output_device().unwrap();
        println!("[Modulator] Output device: {:?}", device.name().unwrap());

        let default_config = device.default_output_config().unwrap();
        let config = SupportedStreamConfig::new(
            1,                       // mono
            SampleRate(sample_rate), // sample rate
            default_config.buffer_size().clone(),
            default_config.sample_format(),
        );

        let output_stream = OutputAudioStream::new(&device, config.clone());

        Modulator {
            carrier_freq,
            sample_rate,
            redundant_periods: REDUNDANT_PERIODS,
            enable_ofdm,
            output_stream,
            config,
            device,
        }
    }

    pub async fn test_carrier_wave(&mut self) {
        // use sin to generate a carrier wave
        let duration = 5.0; // seconds
        let sample_count = (duration * self.sample_rate as f32) as usize;
        let mut wave = vec![];

        for i in 0..sample_count {
            let mut sample = 0.0;
            for freq in &self.carrier_freq {
                sample += (2.0 * std::f64::consts::PI * freq.clone() as f64 * i as f64
                    / self.sample_rate as f64)
                    .sin();
            }
            sample /= self.carrier_freq.len() as f64;
            wave.push(sample as f32);
        }

        println!("[test_carrier_wave] wave length: {:?}", wave.len());

        self.output_stream
            .send(AudioTrack::new(wave.into_iter(), self.config.clone()))
            .await
            .unwrap();
    }

    // [Preamble : 10][Length : 30][Payload : 1024]
    // for each frame:
    //   - split the data into 960 bits for each frame
    //   - get the whole frame bits
    //   - modulate the bits
    //   - send the modulated signal
    // @param data: the input data in compressed u8 format
    // @param len: the length of the input data indicating the number of bits (before compression)
    pub async fn send_bits(&mut self, data: Vec<u8>, len: isize) -> VecDeque<Vec<f32>> {
        // TODO: impl OFDM
        println!("[send_bits] send bits: {:?}", len);
        let mut len = len;
        let mut loop_cnt = 0;
        len -= phy_frame::MAX_FRAME_DATA_LENGTH as isize;

        // for debug
        let mut output = VecDeque::new();

        while len > 0 {
            let mut payload = vec![];
            for i in 0..(phy_frame::MAX_FRAME_DATA_LENGTH / 8) {
                payload.push(data[i + loop_cnt * (phy_frame::MAX_FRAME_DATA_LENGTH / 8)]);
            }
            let frame = phy_frame::PHYFrame::new(phy_frame::MAX_FRAME_DATA_LENGTH, payload);
            let frame_bits = frame.get_whole_frame_bits();
            let decompressed_data = utils::read_compressed_u8_2_data(frame_bits);
            println!(
                "[send_bits] decompressed_data.len(): {}",
                decompressed_data.len()
            );
            let modulated_psk_signal = self.modulate(&decompressed_data, 0);

            // add FSK preamble
            let preamble = phy_frame::gen_preamble(self.sample_rate);
            let mut modulated_signal = preamble.clone();
            modulated_signal.extend(modulated_psk_signal.clone());

            println!(
                "[send_bits] modulated_signal.len(): {}",
                modulated_signal.len()
            );

            // for debug
            output.push_back(modulated_signal.clone());

            self.output_stream
                .send(AudioTrack::new(
                    modulated_signal.into_iter(),
                    self.config.clone(),
                ))
                .await
                .unwrap();
            len -= phy_frame::MAX_FRAME_DATA_LENGTH as isize;
            loop_cnt += 1;
        }

        // send the last frame
        len += phy_frame::MAX_FRAME_DATA_LENGTH as isize;
        println!("[send_bits] remaining len: {:?}", len);
        let mut payload = vec![];
        for i in 0..((len + 7) / 8) {
            payload.push(data[i as usize + loop_cnt * (phy_frame::MAX_FRAME_DATA_LENGTH / 8)]);
        }
        let frame = phy_frame::PHYFrame::new(len as usize, payload);
        let frame_bits = frame.get_whole_frame_bits();
        let decompressed_data = utils::read_compressed_u8_2_data(frame_bits);
        println!(
            "[send_bits] decompressed_data.len(): {}",
            decompressed_data.len()
        );
        let modulated_psk_signal = self.modulate(&decompressed_data, 0);

        // add FSK preamble
        let preamble = phy_frame::gen_preamble(self.sample_rate);
        let mut modulated_signal = preamble.clone();
        modulated_signal.extend(modulated_psk_signal.clone());

        println!(
            "[send_bits] modulated_signal.len(): {}",
            modulated_signal.len()
        );

        // for debug
        output.push_back(modulated_signal.clone());

        self.output_stream
            .send(AudioTrack::new(
                modulated_signal.into_iter(),
                self.config.clone(),
            ))
            .await
            .unwrap();
        println!("[send_bits] send {} frames", loop_cnt + 1);

        // for debug
        return output;
    }

    pub async fn send_bits_2_file(
        &mut self,
        data: Vec<u8>,
        len: isize,
        filename: &str,
    ) -> VecDeque<Vec<f32>> {
        // TODO: impl OFDM
        println!("[send_bits] send bits: {:?}", len);
        let mut len = len;
        let mut loop_cnt = 0;
        len -= phy_frame::MAX_FRAME_DATA_LENGTH as isize;

        // for debug
        let mut output = VecDeque::new();

        // file write use
        let spec = WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = WavWriter::create(filename, spec).unwrap();

        while len > 0 {
            let mut payload = vec![];
            for i in 0..(phy_frame::MAX_FRAME_DATA_LENGTH / 8) {
                payload.push(data[i + loop_cnt * (phy_frame::MAX_FRAME_DATA_LENGTH / 8)]);
            }
            let frame = phy_frame::PHYFrame::new(phy_frame::MAX_FRAME_DATA_LENGTH, payload);
            let frame_bits = frame.get_whole_frame_bits();
            let decompressed_data = utils::read_compressed_u8_2_data(frame_bits);
            println!(
                "[send_bits] decompressed_data.len(): {}",
                decompressed_data.len()
            );
            let modulated_psk_signal = self.modulate(&decompressed_data, 0);

            // add FSK preamble
            let preamble = Modulator::modulate_fsk_preamble();
            let mut modulated_signal = preamble.clone();
            modulated_signal.extend(modulated_psk_signal.clone());
            println!(
                "[send_bits] modulated_signal.len(): {}",
                modulated_signal.len()
            );

            // for debug
            output.push_back(modulated_signal.clone());

            // write to wav file
            for sample in modulated_signal {
                writer.write_sample(sample).unwrap();
            }

            len -= phy_frame::MAX_FRAME_DATA_LENGTH as isize;
            loop_cnt += 1;
        }

        // send the last frame
        len += phy_frame::MAX_FRAME_DATA_LENGTH as isize;
        println!("[send_bits] remaining len: {:?}", len);
        let mut payload = vec![];
        for i in 0..((len + 7) / 8) {
            payload.push(data[i as usize + loop_cnt * (phy_frame::MAX_FRAME_DATA_LENGTH / 8)]);
        }
        let frame = phy_frame::PHYFrame::new(len as usize, payload);
        let frame_bits = frame.get_whole_frame_bits();
        let modulated_psk_signal = self.modulate(&utils::read_compressed_u8_2_data(frame_bits), 0);

        // add FSK preamble
        let preamble = Modulator::modulate_fsk_preamble();
        let mut modulated_signal = preamble.clone();
        modulated_signal.extend(modulated_psk_signal.clone());

        println!(
            "[send_bits] modulated_signal.len(): {}",
            modulated_signal.len()
        );

        // for debug
        output.push_back(modulated_signal.clone());

        // write to wav file
        for sample in modulated_signal {
            writer.write_sample(sample).unwrap();
        }
        println!("[send_bits] send {} frames", loop_cnt + 1);

        writer.finalize().unwrap();

        // for debug
        return output;
    }

    // translate the bits into modulated signal
    pub fn modulate(&self, bits: &Vec<u8>, carrrier_freq_id: usize) -> Vec<f32> {
        // TODO: PSK
        let mut modulated_signal = vec![];
        // redundant periods for each bit
        let sample_cnt_each_bit =
            self.sample_rate * self.redundant_periods / self.carrier_freq[carrrier_freq_id];
        let mut bit_id = 0;
        while bit_id < bits.len() {
            let bit = bits[bit_id];
            let freq = self.carrier_freq[carrrier_freq_id];
            for i in 0..sample_cnt_each_bit {
                let sample = (if bit == 0 { 1.0 } else { -1.0 })
                    * (2.0
                        * std::f64::consts::PI
                        * freq as f64
                        * (i + bit_id as u32 * sample_cnt_each_bit) as f64
                        / self.sample_rate as f64)
                        .sin();
                modulated_signal.push(sample as f32);
            }
            bit_id += 1;
        }
        return modulated_signal;
    }

    pub fn modulate_fsk_preamble() -> Vec<f32> {
        let start = 1e3;
        let end = 1e4;
        let half_length = 400;
        let dx: f64 = 1.0 / 48000.0;
        let step = (end - start) as f64 / half_length as f64;
        let mut fp: Vec<f64> = (0..half_length).map(|i| start + i as f64 * step).collect();
        let fp_rev = fp.clone().into_iter().rev();
        fp.pop();
        fp.extend(fp_rev);

        let mut res = vec![];

        res.push(0.0);
        for i in 1..fp.len() {
            let trap_area = (fp[i] + fp[i - 1]) * dx / 2.0;
            res.push(res[i - 1] + trap_area);
        }

        println!("create preamble len: {}", fp.len());

        res.into_iter()
            .map(|x| (2.0 * std::f64::consts::PI * x).sin() as f32)
            .collect()
    }
}
