use std::time::Duration;

use cpal::{
    traits::{DeviceTrait, HostTrait},
    Device, Host, HostId, SampleRate, SupportedStreamConfig,
};
use futures::{executor::block_on, SinkExt, StreamExt};
use tokio::time;

use crate::asio_stream::{self, AudioTrack};

// Objective 1 (1.5 points): NODE1 should record the TA’s voice for 10 seconds and accurately replay the recorded sound.
async fn obj_1(host: &Host) {
    let device = host
        .default_input_device()
        .expect("failed to get default input device");

    let default_config = device.default_input_config().unwrap();

    let config = SupportedStreamConfig::new(
        1,
        SampleRate(48000),
        default_config.buffer_size().clone(),
        default_config.sample_format(),
    );

    println!("config: {:?}", config);

    let mut input_stream = asio_stream::InputAudioStream::new(&device, config.clone());
    let mut input = vec![];
    println!("start record");
    time::timeout(Duration::from_secs(5), async {
        println!("get into async");
        while let Some(samples) = input_stream.next().await {
            println!("get samples");
            input.extend(samples);
        }
    })
    .await
    .ok();

    println!("start replay");
    let track = AudioTrack::new(input.into_iter(), config.clone());
    let mut output_stream = asio_stream::OutputAudioStream::new(&device, config);
    output_stream.send(track).await.unwrap();
}

// Objective 2 (1.5 points): NODE1 must simultaneously play a predefined sound wave (e.g., a song) and record the playing sound.
// The TA may speak during the recording.
// After 10 seconds, the playback and recording should stop.
// Then, NODE1 must accurately replay the recorded sound.
fn obj_2(host: &Host) {
    let input_device = host
        .default_input_device()
        .expect("failed to get default input device");
}

pub fn pa0() {
    let host = cpal::host_from_id(HostId::Asio).expect("failed to initialise ASIO host");
    block_on(obj_1(&host));
    // obj_2(&host);
}
