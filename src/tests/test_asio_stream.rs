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
        // default_config.channels(),
        1,                 // mono
        SampleRate(48000), // sample rate
        default_config.buffer_size().clone(),
        default_config.sample_format(),
    );

    println!("{:?}", config);

    let mut input1 = InputAudioStream::new(&device, config.clone());

    // let mut input2 = InputAudioStream::new(&device, config);

    let mut data1: Vec<f32> = Vec::with_capacity(64000);
    // let mut data2: Vec<f32> = Vec::with_capacity(64000);

    println!("start listening");
    for _ in 0..2500 {
        let result = input1.next().await.unwrap();
        data1.extend(result.iter());
    }
    println!("listen end");
    // 将data1写入WAV文件
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let mut writer = WavWriter::create("output.wav", spec).unwrap();
    for sample in data1 {
        writer.write_sample(sample).unwrap();
    }
    writer.finalize().unwrap();
    println!("write done");
}
