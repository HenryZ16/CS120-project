// ASIO needed
#![allow(unused_imports)]
use anyhow::{Error, Result};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, FromSample, HostId, Sample, SampleFormat, SampleRate, SizedSample,
    SupportedStreamConfig,
};
use futures::{FutureExt, Sink, SinkExt, Stream, StreamExt};
use hound::{WavSpec, WavWriter};
use rodio::{Decoder, OutputStream, Source};
use std::{
    fs::File,
    io::BufWriter,
    iter::ExactSizeIterator,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use tokio::{
    sync::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        oneshot::{self, Receiver, Sender},
    },
    task, time,
};

mod build_test;
use build_test::build_test;

fn main() {
    build_test();
}
