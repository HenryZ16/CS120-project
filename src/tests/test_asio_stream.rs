use crate::asio_stream::InputAudioStream;
use cpal;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{SampleRate, SupportedStreamConfig};
use futures::StreamExt;
use hound::WavWriter;
use std::i16;

#[tokio::test]
async fn test_input_stream() {
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
        default_config.channels(),
        // 1,                 // mono
        SampleRate(48000), // sample rate
        default_config.buffer_size().clone(),
        default_config.sample_format(),
    );

    println!("{:?}", config);

    let mut input1 = InputAudioStream::new(&device, config.clone());

    let mut input2 = InputAudioStream::new(&device, config);

    let mut data1: Vec<f32> = Vec::with_capacity(64000);
    let mut data2: Vec<f32> = Vec::with_capacity(64000);

    for _ in 0..2000 {
        let result = tokio::join!(input1.next(), input2.next());

        data1.extend(result.0.unwrap().iter());
        data2.extend(result.1.unwrap().iter());
    }

    let error = 1e-4;

    let mut diff_count = 0;
    for i in 0..data1.len() {
        if (data1[i] - data2[i]).abs() > error {
            diff_count += 1;
        }
    }
    // 将data1写入WAV文件
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = WavWriter::create("output.wav", spec).unwrap();
    for sample in data1 {
        let amplitude = (sample * i16::MAX as f32) as i16;
        writer.write_sample(amplitude).unwrap();
    }
    writer.finalize().unwrap();
    println!("diff count: {}", diff_count);
}
