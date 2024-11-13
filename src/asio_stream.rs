use anyhow::{Error, Result};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, FromSample, Sample, SampleFormat, SizedSample,
};
use futures::{FutureExt, Sink, SinkExt, Stream};
use rodio::{OutputStream, Source, SupportedStreamConfig};
use std::{iter::ExactSizeIterator, time::Duration};
use tokio::{
    sync::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        oneshot::{self, Receiver, Sender},
    },
    task,
};

/* struct: AudioTrack<I: ExactSizeIterator>
description: This struct is used to create an audio track.
fields:
- iter: I
- config: SupportedStreamConfig
impl:
- new(
    iter: I,
    config: SupportedStreamConfig
): This function creates a new AudioTrack.
- `Iterator` trait: for field `iter`.
- `Source` trait: for sound record. */
pub struct AudioTrack<I: ExactSizeIterator>
where
    I::Item: rodio::Sample,
{
    iter: I,
    config: SupportedStreamConfig,
}

impl<I: ExactSizeIterator> AudioTrack<I>
where
    I::Item: rodio::Sample,
{
    pub fn new(iter: I, config: SupportedStreamConfig) -> Self {
        return Self { iter, config };
    }

    pub fn len(&self) -> usize {
        return self.iter.len();
    }
}

impl AudioTrack<std::vec::IntoIter<f32>> {
    pub fn from_wav(filename: &str, config: SupportedStreamConfig) -> Self {
        let mut reader = hound::WavReader::open(filename).unwrap();
        let spec = reader.spec();

        println!(
            "Read {filename} with sample format: {} and sample rate: {}",
            match spec.sample_format {
                hound::SampleFormat::Int => match spec.bits_per_sample {
                    8 => "i8",
                    16 => "i16",
                    _ => panic!("unsupported bits per sample"),
                },
                hound::SampleFormat::Float => "f32",
            },
            spec.sample_rate
        );
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Int => match spec.bits_per_sample {
                8 => {
                    let samples: Vec<i8> = reader.samples::<i8>().map(|s| s.unwrap()).collect();
                    samples.iter().map(|&s| s as f32 / i8::MAX as f32).collect()
                }
                16 => {
                    let samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
                    samples
                        .iter()
                        .map(|&s| s as f32 / i16::MAX as f32)
                        .collect()
                }
                _ => panic!("unsupported bits per sample"),
            },
            hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
        };

        return Self::new(samples.clone().into_iter(), config.clone());
    }
}

impl<I: ExactSizeIterator> Iterator for AudioTrack<I>
where
    I::Item: rodio::Sample,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

impl<I: ExactSizeIterator> Source for AudioTrack<I>
where
    I::Item: rodio::Sample,
{
    fn current_frame_len(&self) -> Option<usize> {
        return Some(self.iter.len());
    }

    fn channels(&self) -> u16 {
        return self.config.channels();
    }

    fn sample_rate(&self) -> u32 {
        return self.config.sample_rate().0;
    }

    fn total_duration(&self) -> Option<Duration> {
        return None;
    }
}

/* struct: InputAudioStream
description: This struct is used to create an input audio stream.
fields:
- stream: cpal::Stream
- receiver: UnboundedReceiver<Vec<f32>>
impl:
- new(
    device: &Device,
    config: SupportedStreamConfig
): This function creates a new InputAudioStream.

- `Stream` trait: for field `receiver`.
*/
pub struct InputAudioStream {
    stream: cpal::Stream,
    receiver: UnboundedReceiver<Vec<f32>>,
}

fn build_input_stream<T>(
    device: &Device,
    config: SupportedStreamConfig,
    sender: UnboundedSender<Vec<f32>>,
) -> Result<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let stream = device.build_input_stream(
        &config.config(),
        move |data: &[T], _: &_| {
            let data = data
                .iter()
                .map(|&sample| f32::from_sample(sample))
                .collect::<Vec<f32>>();
            sender.send(data).unwrap();
        },
        |err| eprintln!("an error occurred on stream: {}", err),
        None,
    )?;
    Ok(stream)
}

impl InputAudioStream {
    pub fn new(device: &Device, config: SupportedStreamConfig) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        let stream = match config.sample_format() {
            SampleFormat::I8 => build_input_stream::<i8>(device, config, sender).unwrap(),
            SampleFormat::U8 => build_input_stream::<u8>(device, config, sender).unwrap(),
            SampleFormat::I16 => build_input_stream::<i16>(device, config, sender).unwrap(),
            SampleFormat::U16 => build_input_stream::<u16>(device, config, sender).unwrap(),
            SampleFormat::I32 => build_input_stream::<i32>(device, config, sender).unwrap(),
            SampleFormat::U32 => build_input_stream::<u32>(device, config, sender).unwrap(),
            SampleFormat::F32 => build_input_stream::<f32>(device, config, sender).unwrap(),
            SampleFormat::F64 => build_input_stream::<f64>(device, config, sender).unwrap(),
            SampleFormat::I64 => build_input_stream::<i64>(device, config, sender).unwrap(),
            SampleFormat::U64 => build_input_stream::<u64>(device, config, sender).unwrap(),
            _ => panic!("unsupported sample format"),
        };
        return Self { stream, receiver };
    }
}

impl Stream for InputAudioStream {
    type Item = Vec<f32>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> futures::task::Poll<Option<Self::Item>> {
        self.stream.play().unwrap();
        self.receiver.poll_recv(cx)
    }
}

/* struct: OutputAudioStream
description: This struct is used to create an output audio stream.
fields:
- stream: OutputStream
- sender: UnboundedSender<(AudioTrack<I>, Sender<()>)>
- task: Option<Receiver<()>>
impl:
- new(
    device: &Device,
    config: SupportedStreamConfig
): This function creates a new OutputAudioStream.

- poll(
    mut self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
): This function polls the next item in the stream.

- `Sink` trait: for field `sender`.
*/
pub struct OutputAudioStream<I>
where
    I: ExactSizeIterator + Send + 'static,
    I::Item: rodio::Sample + Send,
    f32: FromSample<I::Item>,
{
    //_stream: OutputStream,
    sender: UnboundedSender<(AudioTrack<I>, Sender<()>)>,
    task: Option<Receiver<()>>,
}

impl<I> OutputAudioStream<I>
where
    I: ExactSizeIterator + Send + 'static,
    I::Item: rodio::Sample + Send,
    f32: FromSample<I::Item>,
{
    pub fn new(device: &Device, config: SupportedStreamConfig) -> Self {
        let device = device.clone();
        let (sender, mut receiver) = mpsc::unbounded_channel::<(AudioTrack<I>, Sender<()>)>();
        task::spawn_blocking(move || {
            while let Some((track, sender)) = receiver.blocking_recv() {
                let (_stream, handle) =
                    OutputStream::try_from_device_config(&device, config.clone()).unwrap();
                let sink = rodio::Sink::try_new(&handle).unwrap();
                sink.append(track);
                sink.sleep_until_end();
                sender.send(()).unwrap();
                std::thread::sleep(Duration::from_millis(15));
            }
        });

        return Self {
            //_stream,
            sender,
            task: None,
        };
    }

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::result::Result<(), Error>> {
        if let Some(ref mut iter) = self.as_mut().task {
            if iter.poll_unpin(cx).is_ready() {
                self.as_mut().task = None;
                return std::task::Poll::Ready(Ok(()));
            } else {
                return std::task::Poll::Pending;
            }
        } else {
            return std::task::Poll::Ready(Ok(()));
        }
    }
}

impl<I> Sink<AudioTrack<I>> for OutputAudioStream<I>
where
    I: ExactSizeIterator + Send + 'static,
    I::Item: rodio::Sample + Send,
    f32: FromSample<I::Item>,
{
    type Error = Error;

    fn poll_close(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::result::Result<(), Error>> {
        self.poll(cx)
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::result::Result<(), Error>> {
        self.poll(cx)
    }

    fn poll_ready(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::result::Result<(), Error>> {
        self.poll(cx)
    }

    fn start_send(
        mut self: std::pin::Pin<&mut Self>,
        item: AudioTrack<I>,
    ) -> std::result::Result<(), Error> {
        let (sender, receiver) = oneshot::channel();
        self.sender.send((item, sender)).unwrap();
        self.as_mut().task = Some(receiver);
        return Ok(());
    }
}

pub fn show_devices() {
    // show available devices
    let host = cpal::host_from_id(cpal::HostId::Asio).expect("failed to initialise ASIO host");
    // let host = cpal::default_host();
    let devices = host.devices().unwrap();
    println!("Available ASIO devices:");
    for device in devices {
        println!("  {:?}", device.name().unwrap());
        let support_config = device.supported_input_configs().unwrap();
        for config in support_config {
            println!("{:?}", config);
        }
    }
}

pub async fn read_wav_into_vec(filename: &str) -> (Vec<f32>, u32) {
    let mut reader = hound::WavReader::open(filename).unwrap();
    let spec = reader.spec();

    println!(
        "Read {filename} with sample format: {} and sample rate: {}",
        match spec.sample_format {
            hound::SampleFormat::Int => match spec.bits_per_sample {
                8 => "i8",
                16 => "i16",
                _ => panic!("unsupported bits per sample"),
            },
            hound::SampleFormat::Float => "f32",
        },
        spec.sample_rate
    );
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => match spec.bits_per_sample {
            8 => {
                let samples: Vec<i8> = reader.samples::<i8>().map(|s| s.unwrap()).collect();
                samples.iter().map(|&s| s as f32 / i8::MAX as f32).collect()
            }
            16 => {
                let samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
                samples
                    .iter()
                    .map(|&s| s as f32 / i16::MAX as f32)
                    .collect()
            }
            _ => panic!("unsupported bits per sample"),
        },
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
    };

    return (samples, spec.sample_rate);
}

pub async fn read_wav(filename: &str) -> (AudioTrack<std::vec::IntoIter<f32>>, u32) {
    use cpal::{
        traits::{DeviceTrait, HostTrait},
        HostId,
    };
    use cpal::{SampleRate, SupportedStreamConfig};

    let (samples, sample_rate) = read_wav_into_vec(filename).await;

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

    return (AudioTrack::new(samples.into_iter(), config), sample_rate);
}

pub async fn read_wav_and_play(filename: &str) {
    use cpal::traits::{DeviceTrait, HostTrait};
    use cpal::{SampleRate, SupportedStreamConfig};
    let (track, sample_rate) = read_wav(filename).await;

    let host = cpal::host_from_id(cpal::HostId::Asio).expect("failed to initialise ASIO host");
    // let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("failed to find output device");
    let default_config = device.default_output_config().unwrap();
    let config = SupportedStreamConfig::new(
        1,                       // mono
        SampleRate(sample_rate), // sample rate
        default_config.buffer_size().clone(),
        default_config.sample_format(),
    );

    let mut output_stream = OutputAudioStream::new(&device, config);
    output_stream.send(track).await.unwrap();
}
