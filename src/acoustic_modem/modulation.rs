/*
Input data
-> Modulation
-> Output Signal
*/

const sampleRate: i32 = 48000;

pub struct Modulator {
    carrier_freq: Vec<u32>,
    carrier: Vec<Vec<f32>>,
    sample_rate: i32,
    enable_OFDM: bool,
    output_stream: OutputAudioStream,
}

impl Modulator {
    pub fn new(carrier_freq: Vec<u32>, sample_rate: i32, enable_OFDM: bool) -> Self {
        use cpal::{
            traits::{DeviceTrait, HostTrait},
            HostId,
        };
        use cpal::{SampleRate, SupportedStreamConfig};
        let (track, sample_rate) = read_wav(filename).await;

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

        let output_stream = OutputAudioStream::new(&device, config);

        Modulator {
            carrier_freq,
            sample_rate,
            enable_OFDM,
        }
    }

    pub fn send(&self, data: Vec<u32>) {}
}
