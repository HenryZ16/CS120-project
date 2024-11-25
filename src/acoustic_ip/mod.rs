use plotters::data;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::oneshot;

use crate::acoustic_mac::controller::{MacController, MacSendTask};

pub mod controller;
pub mod ip_packet;
pub mod protocols;
pub mod receive;
pub mod send;

#[tokio::test]
async fn test_grab_packet() {
    use std::net::Ipv4Addr;
    use std::sync::Arc;
    let ip_addr = Ipv4Addr::new(172, 18, 3, 3);
    let ip_mask = Ipv4Addr::new(255, 255, 0, 0);
    let ip_gateway = Ipv4Addr::new(172, 18, 0, 1);

    let wintun = unsafe { wintun::load_from_path("external\\wintun\\bin\\amd64\\wintun.dll") }
        .expect("Failed to load wintun dll");
    let adapter = match wintun::Adapter::open(&wintun, "AcousticNet") {
        Ok(adapter) => adapter,
        Err(_) => wintun::Adapter::create(&wintun, "AcousticNet", "Wintun", None)
            .expect("Failed to create wintun adapter!"),
    };
    adapter.set_address(ip_addr).unwrap();
    adapter.set_netmask(ip_mask).unwrap();
    adapter.set_gateway(Some(ip_gateway)).unwrap();

    let session = Arc::new(adapter.start_session(wintun::MAX_RING_CAPACITY).unwrap());

    let listen_handle = tokio::task::spawn_blocking(move || loop {
        let packet = session.receive_blocking().unwrap();
        let packet_bytes = packet.bytes();
        if packet_bytes[16] == 239
            && packet_bytes[17] == 255
            && packet_bytes[18] == 255
            && packet_bytes[19] == 250
        {
            continue;
        }
        println!(
            "Length: {:?},Received packet: {:?}",
            packet.bytes().len(),
            packet.bytes()
        );
    });
    let timer_handle = tokio::time::timeout(tokio::time::Duration::from_secs(25), listen_handle);
    let _ = timer_handle.await.unwrap();
}

#[tokio::test]
async fn test_ping() {
    use std::net::Ipv4Addr;
    use std::sync::Arc;
    let ip_addr = Ipv4Addr::new(172, 18, 3, 3);
    let ip_mask = Ipv4Addr::new(255, 255, 0, 0);
    let ip_gateway = Ipv4Addr::new(172, 18, 0, 1);

    let wintun = unsafe { wintun::load_from_path("external\\wintun\\bin\\amd64\\wintun.dll") }
        .expect("Failed to load wintun dll");
    let adapter = wintun::Adapter::create(&wintun, "AcousticNet", "Wintun", None)
        .expect("Failed to create wintun adapter!");
    adapter.set_address(ip_addr).unwrap();
    adapter.set_netmask(ip_mask).unwrap();
    adapter.set_gateway(Some(ip_gateway)).unwrap();

    let session = Arc::new(adapter.start_session(wintun::MAX_RING_CAPACITY).unwrap());

    let listen_handle = tokio::task::spawn_blocking(move || loop {
        let packet = session.receive_blocking().unwrap();
        let mut packet_bytes = packet.bytes().to_vec();
        if packet_bytes[16] == 172
            && packet_bytes[17] == 18
            && packet_bytes[18] == 1
            && packet_bytes[19] == 2
        {
            println!("Received packet: {:?}", packet_bytes);
            packet_bytes.swap(12, 16);
            packet_bytes.swap(13, 17);
            packet_bytes.swap(14, 18);
            packet_bytes.swap(15, 19);

            let mut return_packet = session
                .allocate_send_packet(packet_bytes.len() as u16)
                .unwrap();
            let return_bytes = return_packet.bytes_mut();
            return_bytes.copy_from_slice(&packet_bytes);
            session.send_packet(return_packet);
        }
    });
    let timer_handle = tokio::time::timeout(tokio::time::Duration::from_secs(25), listen_handle);
    let _ = timer_handle.await.unwrap();
}

const CONFIG_FILE: &str = "configuration/pa2.yml";

#[tokio::test]
async fn test_send() {
    let mac_controller = MacController::new(CONFIG_FILE, 0);
    let (send_task_tx, send_task_rx) = unbounded_channel();
    let (recv_task_tx, mut recv_task_rx) = unbounded_channel();

    tokio::spawn(mac_controller.mac_daemon(send_task_rx, recv_task_tx));

    let send_data: Vec<u8> = vec![123; 10];
    let (signal_tx, mut signal_rx) = oneshot::channel();
    let send_task = MacSendTask::new(1, send_data, signal_tx);
    // let mut signal_stream = UnboundedReceiverStream::new(signal_rx);

    if let Ok(_) = send_task_tx.send(send_task) {
        // loop {
        if let Ok(result) = signal_rx.await {
            if result == false {
                println!("send failed");
            } else {
                println!("send successfully");
            }
        }
        // }
    } else {
        println!("Send task pass to Mac failed");
    }
}

#[tokio::test]
async fn test_recv() {
    let mac_controller = MacController::new(CONFIG_FILE, 1);
    let (send_task_tx, send_task_rx) = unbounded_channel();
    let (recv_task_tx, mut recv_task_rx) = unbounded_channel();

    tokio::spawn(mac_controller.mac_daemon(send_task_rx, recv_task_tx));

    while let Some(data) = recv_task_rx.recv().await {
        println!("received data: {:?}", data);
    }
}
