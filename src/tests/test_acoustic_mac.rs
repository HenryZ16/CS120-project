use crate::{
    acoustic_mac::{
        controller::{self, MacController, MacDetector},
        receive,
    },
    utils::get_audio_device_and_config,
};
use std::fs::File;
use std::io::Write;
use tokio::{
    sync::mpsc::unbounded_channel,
    time::{sleep, timeout, Duration, Instant},
};

#[tokio::test]
async fn test_receiver() {
    let config_file = "configuration/pa2.yml";
    let (device, config) = get_audio_device_and_config(48000);
    let mut receiver = receive::MacReceiver::new(config_file, device, config);

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
    let (mut detector, request_rx, result_tx) = MacDetector::new().await;
    let (device, config) = get_audio_device_and_config(48000);
    let detector_daemon = MacDetector::daemon(request_rx, result_tx, device, config);
    // let _ = read_wav_and_play("send.wav");
    let detect_task = tokio::spawn(async move {
        let mut count = 0;
        let instant = Instant::now();
        while instant.elapsed().as_secs() < 10 {
            println!("channel is empty: {}", detector.is_empty().await);
            count += 1;
            let _ = sleep(Duration::from_millis(40)).await;
        }
        println!("detect times: {}", count);
    });

    let _: Result<(), ()> = tokio::select! {
        _ = detector_daemon => {Err(())}
        _ = detect_task => {Ok(())}
    };
}

#[tokio::test]
async fn test_mac_daemon() {
    let mac_controller = MacController::new("configuration/pa2.yml", 1);
    let (send_task_tx, send_task_rx) = unbounded_channel();
    let (recv_task_tx, mut recv_task_rx) = unbounded_channel();

    tokio::spawn(mac_controller.mac_daemon(send_task_rx, recv_task_tx));

    recv_task_rx.recv().await;
}
