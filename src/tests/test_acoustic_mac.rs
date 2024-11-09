use crate::{
    acoustic_mac::{controller::MacDetector, receive},
    asio_stream::read_wav_and_play,
};
use std::fs::File;
use std::io::Write;
use tokio::{
    test,
    time::{sleep, timeout, Duration, Instant},
};

#[tokio::test]
async fn test_receiver() {
    let config_file = "configuration/pa2.yml";

    let mut receiver = receive::MacReceiver::new(config_file);

    let future = receiver.receive_bytes(6250, 1);
    let result = timeout(Duration::from_secs(10), future).await;
    // 将result写入文件
    let mut file = File::create("output.txt").expect("Failed to create file");
    match result {
        Ok(bytes) => {
            file.write_all(&bytes)
                .expect("Failed to write data to file");
            println!("Data written to output.txt");
        }
        Err(e) => {
            println!("Failed to receive bytes: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_detector() {
    let instant = Instant::now();
    let mut detector = MacDetector::new().await;
    // let _ = read_wav_and_play("send.wav");
    let mut count = 0;
    while instant.elapsed().as_secs() < 10 {
        let _ = sleep(Duration::from_millis(200)).await;
        println!("{} ", detector.is_empty().await);
        count += 1;
    }
    println!("detect times: {}", count);
}
