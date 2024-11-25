use anyhow::{Error, Ok, Result};
use tokio::sync::{
    mpsc::{unbounded_channel, UnboundedSender},
    oneshot::{self, Receiver},
};
use tokio_stream::{wrappers::UnboundedReceiverStream, StreamExt};

use crate::{acoustic_mac::controller, generator::ConfigGenerator, utils::Byte};

use super::{
    controller::{MacController, MacSendTask},
    mac_frame::{MACFrame, MacAddress},
};

pub struct NetCard {
    send_task_tx: UnboundedSender<MacSendTask>,
    recv_data_stream: UnboundedReceiverStream<Vec<Byte>>,
}

impl NetCard {
    pub fn new_from_config_file(config_file: &str, mac_address: MacAddress) -> Self {
        let mac_controller = MacController::new(config_file, mac_address);
        let (send_task_tx, send_task_rx) = unbounded_channel();
        let (recv_task_tx, recv_task_rx) = unbounded_channel();
        tokio::spawn(mac_controller.mac_daemon(send_task_rx, recv_task_tx));

        Self {
            send_task_tx,
            recv_data_stream: UnboundedReceiverStream::new(recv_task_rx),
        }
    }

    pub fn new_from_config(config: &ConfigGenerator) -> Self {
        let mac_controller = MacController::new_from_config(config);
        let (send_task_tx, send_task_rx) = unbounded_channel();
        let (recv_task_tx, recv_task_rx) = unbounded_channel();
        tokio::spawn(mac_controller.mac_daemon(send_task_rx, recv_task_tx));

        Self {
            send_task_tx,
            recv_data_stream: UnboundedReceiverStream::new(recv_task_rx),
        }
    }

    pub fn send_unblocked(&self, dst: MacAddress, to_sends: Vec<Byte>) -> Receiver<bool> {
        let (signal_tx, signal_rx) = oneshot::channel();
        let send_task = MacSendTask::new(dst, to_sends, signal_tx);
        let result = self.send_task_tx.send(send_task);
        if result.is_err() {
            println!("[NetCard]: Send error");
        }
        signal_rx
    }

    pub async fn send_async(&self, dst: MacAddress, to_sends: Vec<Byte>) -> Result<bool> {
        let signal_rx = self.send_unblocked(dst, to_sends);
        let result = signal_rx.await;

        if result.is_ok() {
            Ok(result.unwrap())
        } else {
            println!("[NetCard]: Send error");
            Err(Error::msg("[NetCard]: Send error"))
        }
    }

    pub async fn recv_next(&mut self) -> Result<Vec<Byte>> {
        if let Some(data) = self.recv_data_stream.next().await {
            Ok(data)
        } else {
            println!("[NetCard]: Receive error");
            Err(Error::msg("[NetCard]: Receive error"))
        }
    }
}
