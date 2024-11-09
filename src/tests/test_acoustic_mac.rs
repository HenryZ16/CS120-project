use crate::acoustic_mac::{mac_frame, receive};
use crate::acoustic_modem::demodulation;
use crate::acoustic_modem::generator::PhyLayerGenerator;
use plotters::data;
use std::fs::File;
use std::io::Write;
use tokio::sync::mpsc::unbounded_channel;
use tokio::{test, time::timeout, time::Duration};

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
