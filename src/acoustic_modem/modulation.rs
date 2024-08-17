/*
Input data
-> Modulation
-> Output Signal
*/
use crate::asio_stream::{read_wav_into_vec, AudioTrack, OutputAudioStream};
use crate::symrs::utils::lcm;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{HostId, SampleRate, SupportedStreamConfig};
use futures::executor::block_on;
use futures::SinkExt;

use super::phy_frame;

const SAMPLE_RATE: u32 = 48000;

pub struct Modulator {
    carrier_freq: Vec<u32>,
    sample_rate: u32,
    enable_ofdm: bool,
    output_stream: OutputAudioStream<std::vec::IntoIter<f32>>,
    config: SupportedStreamConfig,
}

impl Modulator {
    pub fn new(carrier_freq: Vec<u32>, sample_rate: u32, enable_ofdm: bool) -> Self {
        let host = cpal::host_from_id(HostId::Asio).expect("failed to initialise ASIO host");
        let device = host.output_devices().expect("failed to find output device");
        let device = device
            .into_iter()
            .next()
            .expect("no output device available");
        println!("Output device: {:?}", device.name().unwrap());

        let default_config = device.default_input_config().unwrap();
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
            enable_ofdm,
            output_stream,
            config,
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

        println!("wave length: {:?}", wave.len());

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
    pub async fn send_bits(&mut self, data: Vec<u8>, len: isize) {
        // TODO
        let mut len = len;
        while len > 0 {
            len -= phy_frame::MAX_FRAME_DATA_LENGTH as isize;
            let mut payload = vec![];
            for i in 0..phy_frame::MAX_FRAME_DATA_LENGTH {
                payload.push(data[i]);
            }
            let frame = phy_frame::PHYFrame::new(phy_frame::MAX_FRAME_DATA_LENGTH, payload);
            let frame_bits = frame.get_whole_frame_bits();
            let modulated_signal = self.modulate(&frame_bits);
            self.output_stream
                .send(AudioTrack::new(
                    modulated_signal.into_iter(),
                    self.config.clone(),
                ))
                .await
                .unwrap();
        }

        // send the last frame
        let mut payload = vec![];
        for i in 0..len {
            payload.push(data[i as usize]);
        }
        let frame = phy_frame::PHYFrame::new(len as usize, payload);
        let frame_bits = frame.get_whole_frame_bits();
        let modulated_signal = self.modulate(&frame_bits);
        self.output_stream
            .send(AudioTrack::new(
                modulated_signal.into_iter(),
                self.config.clone(),
            ))
            .await
            .unwrap();
    }

    pub fn modulate(&self, bits: &Vec<u8>) -> Vec<f32> {
        // TODO: PSK
        vec![0.3]
    }
}
