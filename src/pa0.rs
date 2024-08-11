use cpal::{
    traits::{DeviceTrait, HostTrait},
    Device, HostId, SampleRate, SupportedStreamConfig,
};

use crate::asio_stream;

// Objective 1 (1.5 points): NODE1 should record the TA’s voice for 10 seconds and accurately replay the recorded sound.
fn obj_1(device: &Device) {
    let default_config = device.default_input_config().unwrap();
    let config = SupportedStreamConfig::new(
        1,                 // mono
        SampleRate(48000), // sample rate
        default_config.buffer_size().clone(),
        default_config.sample_format(),
    );
}

// Objective 2 (1.5 points): NODE1 must simultaneously play a predefined sound wave (e.g., a song) and record the playing sound.
// The TA may speak during the recording.
// After 10 seconds, the playback and recording should stop.
// Then, NODE1 must accurately replay the recorded sound.
fn obj_2(device: &Device) {
    let default_config = device.default_input_config().unwrap();
    let config = SupportedStreamConfig::new(
        1,                 // mono
        SampleRate(48000), // sample rate
        default_config.buffer_size().clone(),
        default_config.sample_format(),
    );
}

pub fn pa0() {
    let host = cpal::host_from_id(HostId::Asio).expect("failed to initialise ASIO host");
    let device = host
        .default_input_device()
        .expect("failed to find input device");
    obj_1(&device);
    obj_2(&device);
}
