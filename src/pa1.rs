use anyhow::{Error, Result};
use cpal::{
    traits::{DeviceTrait, HostTrait},
    Device, Host, HostId, SampleRate, SupportedStreamConfig,
};

pub async fn obj_1(host: &Host) {}

pub async fn pa1(sel: u32) -> Result<u32> {
    let available_sel = vec![0, 1];
    if !available_sel.contains(&sel) {
        return Err(Error::msg("Invalid selection"));
    }

    let host = cpal::host_from_id(HostId::Asio).expect("failed to initialise ASIO host");

    if sel == 0 || sel == 1 {
        println!("Objective 1 start");
        obj_1(&host).await;
        println!("Objective 1 end");
    }

    return Ok(0);
}
