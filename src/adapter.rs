use crate::acoustic_ip::adapter::{self, Adapter};
use tokio::time::{sleep, Duration};

pub async fn adapter_task() {
    let config_file = "configuration/pa3.yml";

    let adapter = Adapter::new_from_config(config_file);
    let adapter_task = Adapter::start_daemon(adapter).await;
    sleep(Duration::from_secs(60)).await;
    Adapter::stop_daemon(adapter_task).await;
}
