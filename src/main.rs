mod asio_stream;
mod pa0;
mod symrs;
mod tests;

#[tokio::main]
async fn main() {
    pa0::pa0().await;
    // block_on(asio_stream::read_wav_and_play("./audio/hallelujah.wav"));
}
