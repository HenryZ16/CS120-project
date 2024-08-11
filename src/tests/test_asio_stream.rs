use crate::asio_stream::read_wav_and_play;

#[tokio::test]
async fn test_asio_output_stream() {
    read_wav_and_play("audio/hallelujah.wav").await;
}
