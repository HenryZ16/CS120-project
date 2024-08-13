use anyhow::{Error, Result};
use cpal::{
    traits::{DeviceTrait, HostTrait},
    Device, Host, HostId, SampleRate, SupportedStreamConfig,
};
use futures::{executor::block_on, future::join, join, SinkExt, StreamExt};
use std::time::{Duration, Instant};
use tokio::time;

use crate::asio_stream::{self, AudioTrack};

// Objective 1 (1.5 points): NODE1 should record the TA’s voice for 10 seconds and accurately replay the recorded sound.
async fn obj_1(host: &Host) {
    let input_device = host
        .default_input_device()
        .expect("failed to get default input device");

    let output_device = host
        .default_output_device()
        .expect("failed to get default output device");

    let default_config = input_device.default_input_config().unwrap();

    let config = SupportedStreamConfig::new(
        1,
        SampleRate(48000),
        default_config.buffer_size().clone(),
        default_config.sample_format(),
    );

    println!("config: {:?}", config);

    let mut input_stream = asio_stream::InputAudioStream::new(&input_device, config.clone());
    let mut input = vec![];
    println!("start record");
    let start = Instant::now();

    time::timeout(Duration::from_secs(10), async {
        while let Some(samples) = input_stream.next().await {
            input.extend(samples);
        }
    })
    .await
    .ok();

    let duration = start.elapsed();
    println!("Time elapsed in recording is: {:?}", duration);
    println!("Length of input: {}", input.len());

    println!("start replay");
    let track = AudioTrack::new(input.into_iter(), config.clone());
    let mut output_stream = asio_stream::OutputAudioStream::new(&output_device, config);

    let start = Instant::now();
    output_stream.send(track).await.unwrap();
    let duration = start.elapsed();
    println!("Time elapsed in replaying is: {:?}", duration);
}

// Objective 2 (1.5 points): NODE1 must simultaneously play a predefined sound wave (e.g., a song) and record the playing sound.
// The TA may speak during the recording.
// After 10 seconds, the playback and recording should stop.
// Then, NODE1 must accurately replay the recorded sound.
async fn obj_2(host: &Host) {
    let filename = "audio/hallelujah.wav";

    let input_device = host
        .default_input_device()
        .expect("failed to get default input device");

    let output_device = host
        .default_output_device()
        .expect("failed to get default output device");

    let default_config = input_device.default_input_config().unwrap();

    let config = SupportedStreamConfig::new(
        1,
        SampleRate(48000),
        default_config.buffer_size().clone(),
        default_config.sample_format(),
    );

    let mut input_stream = asio_stream::InputAudioStream::new(&input_device, config.clone());
    let mut input = vec![];

    println!("start playing");
    let output_handle = asio_stream::read_wav_and_play(filename);

    println!("start record");
    let start = Instant::now();
    let input_handle = time::timeout(Duration::from_secs(10), async {
        while let Some(samples) = input_stream.next().await {
            input.extend(samples);
        }
    });

    join!(output_handle, input_handle);

    let duration = start.elapsed();

    println!("Time elapsed in recording is: {:?}", duration);

    println!("start replay");
    let mut output_stream = asio_stream::OutputAudioStream::new(&output_device, config.clone());
    let track = AudioTrack::new(input.into_iter(), config.clone());
    let start = Instant::now();
    output_stream.send(track).await.unwrap();
    let duration = start.elapsed();
    println!("Time elapsed in replaying is: {:?}", duration);
}

pub async fn pa0(sel: u32) -> Result<u32> {
    let host = cpal::default_host();
    let available_sel = vec![0, 1, 2];
    if !available_sel.contains(&sel) {
        return Err(Error::msg("Invalid selection"));
    }

    if sel == 0 || sel == 1 {
        println!("Objective 1 start");
        obj_1(&host).await;
        println!("Objective 1 end");
    }

    if sel == 0 || sel == 2 {
        println!("Objective 2 start");
        obj_2(&host).await;
        println!("Objective 2 end");
    }

    return Ok(0);
}
