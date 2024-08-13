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

const SAMPLE_RATE: u32 = 48000;

pub struct Modulator {
    carrier_freq: Vec<u32>,
    carrier: Vec<Vec<f32>>,
    sample_rate: u32,
    enable_ofdm: bool,
    output_stream: OutputAudioStream<std::vec::IntoIter<f32>>,
    config: SupportedStreamConfig,
}

impl Modulator {
    pub fn new(carrier_freq: Vec<u32>, sample_rate: u32, enable_ofdm: bool) -> Self {
        let mut carrier = vec![];
        for c in &carrier_freq {
            let name = format!("audio/freq-{}.wav", c);
            let (samples, rate) = block_on(read_wav_into_vec(&name));
            assert_eq!(rate, SAMPLE_RATE);
            carrier.push(samples);
        }

        let host = cpal::host_from_id(HostId::Asio).expect("failed to initialise ASIO host");
        let device = host
            .default_input_device()
            .expect("failed to find input device");
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
            carrier,
            sample_rate,
            enable_ofdm,
            output_stream,
            config,
        }
    }

    pub async fn test_wave(&mut self) {
        let mut lcm_freq = 1;
        for c in &self.carrier_freq {
            lcm_freq = lcm(lcm_freq, *c as u64);
        }
        let mut wave = vec![0.0; lcm_freq as usize];
        for c in &self.carrier {
            for i in 0..wave.len() {
                wave[i] += c[i % c.len()];
            }
        }

        let duration = 5;
        let max_len: usize = (self.sample_rate * duration) as usize;
        let mut wave = wave.into_iter().cycle().take(max_len).collect::<Vec<_>>();

        self.output_stream
            .send(AudioTrack::new(wave.into_iter(), self.config.clone()))
            .await
            .unwrap();
    }

    pub fn send_bits(&self, data: Vec<u32>, len: usize) {
        // TODO: fit in the PHY frame in pa1 - obj2
    }
}
