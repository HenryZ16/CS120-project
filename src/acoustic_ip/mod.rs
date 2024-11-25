pub mod controller;
pub mod ip_packet;
pub mod receive;
pub mod send;

#[test]
fn test_grab_ping_packet() {
    use std::sync::Arc;

    let wintun = unsafe { wintun::load_from_path("external\\wintun\\bin\\amd64\\wintun.dll") }
        .expect("Failed to load wintun dll");
    let adapter = wintun::Adapter::create(&wintun, "ANet0", "Wintun", None)
        .expect("Failed to create wintun adapter!");
    let session = Arc::new(adapter.start_session(wintun::MAX_RING_CAPACITY).unwrap());

    let listen_handle = tokio::task::spawn_blocking({
        let session = session.clone();
        move || {
            while let Ok(packet) = session.receive_blocking() {
                println!("Received packet: {:?}", packet.bytes());
            }
        }
    });
    let timer_handle = tokio::time::timeout(tokio::time::Duration::from_secs(25), listen_handle);
    let _ = futures::executor::block_on(timer_handle);
}
