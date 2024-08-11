use anyhow::Error;
use cpal::{
    traits::{DeviceTrait, StreamTrait},
    Device, FromSample, Sample, SampleFormat,
};
use futures::{FutureExt, Sink, Stream};
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

impl InputAudioStream {
    pub fn new(device: &Device, config: SupportedStreamConfig) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        if let SampleFormat::I8 = config.sample_format() {
            let stream = device
                .build_input_stream(
                    &config.config(),
                    move |data: &[i8], _: &_| {
                        let data = data
                            .iter()
                            .map(|&x| f32::from_sample(x))
                            .collect::<Vec<f32>>();
                        sender.send(data).unwrap();
                    },
                    move |err| {
                        eprintln!("an error occurred on stream: {}", err);
                    },
                    None,
                )
                .unwrap();
            return Self { stream, receiver };
        } else {
            panic!("Sample format is not I8");
        }
    }
}

impl Stream for InputAudioStream {
    type Item = Vec<f32>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.stream.play().unwrap();
        self.receiver.poll_recv(cx)
    }
}

/* struct: OutputAudioStream
description: This struct is used to create an output audio stream.
fields:
- stream: OutputStream
- sender: UnboundedSender<f32>
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
    _stream: OutputStream,
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
        let (_stream, handle) = OutputStream::try_from_device_config(device, config).unwrap();
        let (sender, mut receiver) = mpsc::unbounded_channel::<(AudioTrack<I>, Sender<()>)>();
        let sink = rodio::Sink::try_new(&handle).unwrap();

        task::spawn_blocking(move || {
            while let Some((track, sender)) = receiver.blocking_recv() {
                sink.append(track);
                sink.sleep_until_end();
                sender.send(()).unwrap();
            }
        });

        return Self {
            _stream,
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
