use std::collections::VecDeque;
use std::vec;

/*
Input data
-> Modulation
-> Output Signal
*/
use super::phy_frame::{self, PHYFrame};
use crate::asio_stream::{AudioTrack, OutputAudioStream};
use crate::utils::{self, Byte};
use cpal::SupportedStreamConfig;
use futures::SinkExt;
use hound::{WavSpec, WavWriter};

pub const SAMPLE_RATE: u32 = 48000;
pub const OFDM_FRAME_DISTANCE: usize = 30;
pub const REDUNDANT_PERIODS: usize = 1;
pub const ENABLE_ECC: bool = false;

pub struct Modulator {
    carrier_freq: Vec<u32>,
    sample_rate: u32,
    redundant_periods: usize,
    enable_ofdm: bool,
    output_stream: OutputAudioStream<std::vec::IntoIter<f32>>,
    config: SupportedStreamConfig,
}

impl Modulator {
    pub fn new(
        carrier_freq: Vec<u32>,
        device: cpal::Device,
        config: SupportedStreamConfig,
        enable_ofdm: bool,
    ) -> Self {
        let output_stream = OutputAudioStream::new(&device, config.clone());
        let sample_rate = config.sample_rate().0;

        Modulator {
            carrier_freq,
            sample_rate,
            redundant_periods: REDUNDANT_PERIODS,
            enable_ofdm,
            output_stream,
            config,
        }
    }

    pub fn get_carrier_cnt(&self) -> usize {
        self.carrier_freq.len()
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
            sample /= self.get_carrier_cnt() as f64;
            wave.push(sample as f32);
        }

        println!("[test_carrier_wave] wave length: {:?}", wave.len());

        self.output_stream
            .send(AudioTrack::new(wave.into_iter(), self.config.clone()))
            .await
            .unwrap();
    }

    // data: compressed
    // data_bits_len: the total bits cnt of the data
    pub async fn bits_2_wave_single_ofdm_frame_no_ecc(
        &mut self,
        data: Vec<Byte>,
        data_bits_len: usize,
    ) -> Vec<f32> {
        // let data_bits_len = data_bits_len as usize;
        let carrier_cnt = self.get_carrier_cnt();
        assert!(data_bits_len <= carrier_cnt * phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING);

        // warm up
        // let mut modulated_signal: Vec<f32> = (0..500)
        //     .map(|x| (std::f64::consts::PI * x as f64 / 6.0).sin() as f32)
        //     .collect();

        // fill up the payload
        let mut data = data;
        let mut data_bits_len = data_bits_len;
        // while data.len() * 8 < phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING * carrier_cnt {
        //     data.push(0);
        // }

        // modulate the data for each carrier
        let mut modulated_psk_signal: Vec<f32> = vec![];
        let mut max_len = 0;
        for i in 0..carrier_cnt {
            let (payload, phy_len) = if data_bits_len > 0 {
                // start, end: ptr to decompressed u8
                let data_start = i * phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING;
                let data_end = if data_bits_len >= phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING {
                    (i + 1) * phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING
                } else {
                    data.len() * 8
                };
                let mut real_end = data_end;
                while real_end - data_start < max_len {
                    data.push(0);
                    real_end += 8;
                }

                (
                    data[(data_start >> 3)..(real_end >> 3)].to_vec(),
                    data_end - data_start,
                )
            } else {
                (vec![0; max_len], 0)
            };
            data_bits_len -= phy_len;
            max_len = if phy_len > max_len { phy_len } else { max_len };

            // println!(
            //     "[bits_2_wave_single_ofdm_frame_no_ecc] phy_len: {}, payload.len(): {:?} for carrier {}",
            //     phy_len,
            //     payload.len(),
            //     i
            // );
            let frame = phy_frame::PHYFrame::new_no_encoding(phy_len, payload);
            let frame_bits = PHYFrame::add_crc(frame.1);
            // println!(
            //     "[bits_2_wave_single_ofdm_frame_no_ecc] frame_bits: {:?}",
            //     frame_bits
            // );
            let decompressed_data = utils::read_compressed_u8_2_data(frame_bits);
            modulated_psk_signal = if i == 0 {
                self.modulate(&decompressed_data, i)
            } else {
                let modulated_psk_signal_i = self.modulate(&decompressed_data, i);
                modulated_psk_signal
                    .iter()
                    .zip(modulated_psk_signal_i.iter())
                    .map(|(a, b)| a + b)
                    .collect()
            };
        }

        modulated_psk_signal = modulated_psk_signal
            .iter()
            .map(|&x| x / carrier_cnt as f32)
            .collect();

        // add FSK preamble
        let mut modulated_signal = phy_frame::gen_preamble(self.sample_rate);
        modulated_signal.extend(modulated_psk_signal);
        return modulated_signal;
    }

    pub async fn bits_2_wave(&mut self, data: Vec<Byte>, len: isize) -> Vec<f32> {
        let mut modulated_signal: Vec<f32> = vec![];
        let mut len = len;
        let mut loop_cnt = 0;

        if !self.enable_ofdm {
            len -= phy_frame::MAX_FRAME_DATA_LENGTH as isize;

            while len > 0 {
                let mut payload = vec![];
                for i in 0..(phy_frame::MAX_FRAME_DATA_LENGTH / 8) {
                    payload.push(data[i + loop_cnt * (phy_frame::MAX_FRAME_DATA_LENGTH / 8)]);
                }
                println!("push in payload data: {:?}", payload);
                println!("frame len: {}", phy_frame::MAX_FRAME_DATA_LENGTH);
                let frame =
                    phy_frame::PHYFrame::new_no_encoding(phy_frame::MAX_FRAME_DATA_LENGTH, payload);
                let frame_bits = frame.1;
                let decompressed_data = utils::read_compressed_u8_2_data(frame_bits);
                // println!(
                //     "[bits_2_wave] decompressed_data.len(): {}",
                //     decompressed_data.len()
                // );
                let modulated_psk_signal = self.modulate(&decompressed_data, 0);

                // add FSK preamble
                let preamble = phy_frame::gen_preamble(self.sample_rate);
                modulated_signal.extend(preamble.clone());
                modulated_signal.extend(modulated_psk_signal.clone());

                // println!(
                //     "[bits_2_wave] modulated_signal.len(): {}",
                //     modulated_signal.len()
                // );

                len -= phy_frame::MAX_FRAME_DATA_LENGTH as isize;
                loop_cnt += 1;

                // wait for a while
                // tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                modulated_signal.extend(vec![0.0; 500]);
            }

            // send the last frame
            len += phy_frame::MAX_FRAME_DATA_LENGTH as isize;
            println!("[bits_2_wave] remaining len: {:?}", len);
            let mut payload = vec![];
            for i in 0..((len + 7) / 8) {
                payload.push(data[i as usize + loop_cnt * (phy_frame::MAX_FRAME_DATA_LENGTH / 8)]);
            }
            println!("push in payload data: {:?}", payload);
            println!("frame len: {}", len);
            let frame = phy_frame::PHYFrame::new_no_encoding(len as usize, payload);
            let frame_bits = frame.1;
            let decompressed_data = utils::read_compressed_u8_2_data(frame_bits);
            // println!(
            //     "[bits_2_wave] decompressed_data.len(): {}",
            //     decompressed_data.len()
            // );
            let modulated_psk_signal = self.modulate(&decompressed_data, 0);

            // add FSK preamble
            let preamble = phy_frame::gen_preamble(self.sample_rate);
            modulated_signal.extend(preamble.clone());
            modulated_signal.extend(modulated_psk_signal.clone());

            // println!(
            //     "[bits_2_wave] modulated_signal.len(): {}",
            //     modulated_signal.len()
            // );
            // println!("[bits_2_wave] send {} frames", loop_cnt + 1);
        } else {
            // OFDM
            let carrier_cnt = self.get_carrier_cnt();
            if !ENABLE_ECC {
                len -= (phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING * carrier_cnt) as isize;
            } else {
                len -= (phy_frame::MAX_FRAME_DATA_LENGTH * carrier_cnt) as isize;
            }
            while len > 0 {
                let mut modulated_psk_signal: Vec<f32> = vec![];

                for i in 0..carrier_cnt {
                    let mut payload = vec![];
                    if !ENABLE_ECC {
                        for j in 0..(phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING / 8) {
                            payload.push(
                                data[j
                                    + (loop_cnt * carrier_cnt + i)
                                        * (phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING / 8)],
                            );
                        }
                    } else {
                        for j in 0..(phy_frame::MAX_FRAME_DATA_LENGTH / 8) {
                            payload.push(
                                data[j
                                    + (loop_cnt * carrier_cnt + i)
                                        * (phy_frame::MAX_FRAME_DATA_LENGTH / 8)],
                            );
                        }
                    }

                    let mut decompressed_data = vec![];

                    if !ENABLE_ECC {
                        let frame = phy_frame::PHYFrame::new_no_encoding(
                            phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING,
                            payload,
                        );
                        let frame_bits = PHYFrame::add_crc(frame.1);
                        decompressed_data = utils::read_compressed_u8_2_data(frame_bits);
                    } else {
                        let frame =
                            phy_frame::PHYFrame::new(phy_frame::MAX_FRAME_DATA_LENGTH, payload);
                        let frame_bits = phy_frame::PHYFrame::get_whole_frame_bits(&frame);
                        decompressed_data = utils::read_compressed_u8_2_data(frame_bits);
                    }

                    let modulated_psk_signal_i = self.modulate(&decompressed_data, i);

                    // nomalization - make the power of each carrier equal
                    let modulated_psk_signal_i: Vec<f32> = modulated_psk_signal_i
                        .iter()
                        .map(|&x| {
                            x / ((self.carrier_freq[i] as f32 / self.carrier_freq[0] as f32)
                                .powf(2.0)) as f32
                        })
                        .collect();

                    if i == 0 {
                        modulated_psk_signal.extend(modulated_psk_signal_i.clone());
                    } else {
                        modulated_psk_signal = modulated_psk_signal
                            .iter()
                            .zip(modulated_psk_signal_i.iter())
                            .map(|(a, b)| a + b)
                            .collect();
                    }
                }

                // nomalization - make the maximum of the sequence equal to 1
                let divisor = (1..(carrier_cnt + 1)).fold(0.0, |acc, x| {
                    acc + 1.0
                        / ((self.carrier_freq[x - 1] as f32 / self.carrier_freq[0] as f32)
                            .powf(2.0)) as f32
                });
                modulated_psk_signal = modulated_psk_signal
                    .iter()
                    .map(|&x| x / divisor as f32)
                    .collect();

                // add FSK preamble
                let preamble = phy_frame::gen_preamble(self.sample_rate);
                modulated_signal.extend(preamble.clone());
                modulated_signal.extend(modulated_psk_signal.clone());

                if !ENABLE_ECC {
                    len -= (phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING * carrier_cnt) as isize;
                } else {
                    len -= (phy_frame::MAX_FRAME_DATA_LENGTH * carrier_cnt) as isize;
                }
                loop_cnt += 1;

                // wait for a while
                modulated_signal.extend(vec![0.0; OFDM_FRAME_DISTANCE]);
            }

            // send the last frame
            if !ENABLE_ECC {
                len += (phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING * carrier_cnt) as isize;
            } else {
                len += (phy_frame::MAX_FRAME_DATA_LENGTH * carrier_cnt) as isize;
            }
            let mut modulated_psk_signal: Vec<f32> = vec![];
            for i in 0..carrier_cnt {
                let mut payload = vec![];
                let mut bit_len = 0;
                if !ENABLE_ECC {
                    bit_len = if len > phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING as isize {
                        phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING
                    } else {
                        len as usize
                    };
                    let frame_len = (bit_len + 7) / 8;
                    if len > 0 {
                        for j in 0..frame_len {
                            payload.push(
                                data[j as usize
                                    + (loop_cnt * carrier_cnt + i)
                                        * (phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING / 8)],
                            );
                        }
                    }
                    // println!("payload: {:?}, length: {}", payload, len);
                } else {
                    bit_len = if len > phy_frame::MAX_FRAME_DATA_LENGTH as isize {
                        phy_frame::MAX_FRAME_DATA_LENGTH
                    } else {
                        len as usize
                    };
                    let frame_len = (bit_len + 7) / 8;
                    if len > 0 {
                        for j in 0..frame_len {
                            payload.push(
                                data[j as usize
                                    + (loop_cnt * carrier_cnt + i)
                                        * (phy_frame::MAX_FRAME_DATA_LENGTH / 8)],
                            );
                        }
                    }
                }
                let mut decompressed_data = vec![];

                if !ENABLE_ECC {
                    let frame = phy_frame::PHYFrame::new_no_encoding(bit_len, payload);
                    let frame_bits = PHYFrame::add_crc(frame.1);
                    decompressed_data = utils::read_compressed_u8_2_data(frame_bits);
                } else {
                    let frame = phy_frame::PHYFrame::new(bit_len, payload);
                    let frame_bits = phy_frame::PHYFrame::get_whole_frame_bits(&frame);
                    decompressed_data = utils::read_compressed_u8_2_data(frame_bits);
                }
                let modulated_psk_signal_i = self.modulate(&decompressed_data, i);

                if i == 0 {
                    modulated_psk_signal.extend(modulated_psk_signal_i.clone());
                } else {
                    modulated_psk_signal = modulated_psk_signal
                        .iter()
                        .zip(modulated_psk_signal_i.iter())
                        .map(|(a, b)| a + b)
                        .collect();
                }
                if !ENABLE_ECC {
                    len -= phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING as isize;
                } else {
                    len -= phy_frame::MAX_FRAME_DATA_LENGTH as isize;
                }
                if len < 0 {
                    len = 0;
                }
            }

            // add FSK preamble
            let preamble = phy_frame::gen_preamble(self.sample_rate);
            modulated_signal.extend(preamble.clone());
            modulated_signal.extend(modulated_psk_signal.clone());
        }
        // cool down
        let cool_down_vec: Vec<f32> = vec![0.0; 30];
        modulated_signal.extend(cool_down_vec);

        println!(
            "[bits_2_wave] modulated_signal.len(): {}",
            modulated_signal.len()
        );
        return modulated_signal;
    }

    pub async fn send_modulated_signal(&mut self, modulated_signal: Vec<f32>) {
        self.output_stream
            .send(AudioTrack::new(
                modulated_signal.into_iter(),
                self.config.clone(),
            ))
            .await
            .unwrap();
    }

    pub async fn send_single_ofdm_frame(&mut self, data: Vec<Byte>, len: isize) {
        // data here is compressed u8
        let modulated_signal = self
            .bits_2_wave_single_ofdm_frame_no_ecc(data, len as usize)
            .await;

        self.output_stream
            .send(AudioTrack::new(
                modulated_signal.into_iter(),
                self.config.clone(),
            ))
            .await
            .unwrap();
        // TODO: remove it after implementing the controller
        // tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
    }

    // [Preamble : 8][Payload : 36 x 6 = 216]
    // for each frame:
    //   - split the data into 96 bits for each frame
    //   - get the whole frame bits
    //   - modulate the bits
    //   - send the modulated signal
    // @param data: the input data in compressed u8 format
    // @param len: the length of the input data indicating the number of bits (before compression)
    pub async fn send_bits(&mut self, data: Vec<Byte>, len: isize) -> VecDeque<Vec<f32>> {
        // for debug
        let mut output = VecDeque::new();

        let modulated_signal: Vec<f32> = self.bits_2_wave(data, len).await;

        // for debug
        output.push_back(modulated_signal.clone());

        self.output_stream
            .send(AudioTrack::new(
                modulated_signal.into_iter(),
                self.config.clone(),
            ))
            .await
            .unwrap();

        // for debug
        return output;
    }

    pub async fn send_bits_2_file(
        &mut self,
        data: Vec<u8>,
        len: isize,
        filename: &str,
    ) -> VecDeque<Vec<f32>> {
        // for debug
        let mut output = VecDeque::new();

        let modulated_signal: Vec<f32> = self.bits_2_wave(data, len).await;

        // for debug
        output.push_back(modulated_signal.clone());

        // file write use
        let spec = WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = WavWriter::create(filename, spec).unwrap();

        // write to wav file
        for sample in modulated_signal {
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();

        // for debug
        return output;
    }

    // translate the bits into modulated signal
    pub fn modulate(&self, bits: &Vec<u8>, carrrier_freq_id: usize) -> Vec<f32> {
        // println!("output: {:?}, length: {}", bits, bits.len());

        let mut modulated_signal = vec![];
        // redundant periods for each bit
        let sample_cnt_each_bit =
            self.sample_rate * self.redundant_periods as u32 / self.carrier_freq[1];
        let mut bit_id = 0;
        while bit_id < bits.len() {
            let bit = bits[bit_id];
            let freq = self.carrier_freq[carrrier_freq_id];
            for i in 0..sample_cnt_each_bit {
                let sample = (if bit == 0 {
                    1.0
                } else if bit == 1 {
                    -1.0
                } else {
                    0.0
                }) * if carrrier_freq_id == 0 {
                    (2.0 * std::f64::consts::PI
                        * freq as f64
                        * (i + bit_id as u32 * sample_cnt_each_bit) as f64
                        / self.sample_rate as f64)
                        .sin()
                        .abs()
                } else {
                    (2.0 * std::f64::consts::PI
                        * freq as f64
                        * (i + bit_id as u32 * sample_cnt_each_bit) as f64
                        / self.sample_rate as f64)
                        .sin()
                };
                modulated_signal.push(sample as f32);
            }
            bit_id += 1;
        }
        return modulated_signal;
    }
}

#[tokio::test]
async fn test_b2w_single_ofdm_frame() {
    let data_len = 2 * phy_frame::MAX_FRAME_DATA_LENGTH_NO_ENCODING - 3;
    let data: Vec<u8> = (0..data_len).map(|x| (x & 1) as u8).collect();

    // modulator
    let sample_rate = 48000;
    let (device, config) = utils::get_audio_device_and_config(sample_rate);
    let carrier_freq = 6000;
    let mut modulator = Modulator::new(vec![carrier_freq, carrier_freq * 2], device, config, true);

    // bits 2 wave single ofdm frame
    let modulated_signal = modulator
        .bits_2_wave_single_ofdm_frame_no_ecc(
            utils::read_data_2_compressed_u8(data.clone()),
            data_len,
        )
        .await;

    // write to wav file
    let filename = "testset/b2w_test.wav";
    let spec = WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = WavWriter::create(filename, spec).unwrap();
    for sample in modulated_signal {
        writer.write_sample(sample).unwrap();
    }
    writer.finalize().unwrap();
}
