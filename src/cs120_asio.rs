use rodio::Decoder;
use std::fs::File;

pub fn get_wav_source(src_name: &str) -> Decoder<File> {
    let src = File::open(src_name).unwrap();
    Decoder::new(src).unwrap()
}

pub fn play_wav_until_end(src_name: &str) {
    let source = get_wav_source(src_name);
    let (_stream, stream_handle) = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::Sink::try_new(&stream_handle).unwrap();
    sink.append(source);
    sink.sleep_until_end();
}
