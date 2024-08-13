use crate::acoustic_modem::modulation::Modulator;
use crate::pa0;
use anyhow::{Error, Result};
use cpal::{
    traits::{DeviceTrait, HostTrait},
    Device, Host, HostId, SampleRate, SupportedStreamConfig,
};

pub async fn obj_2(host: &Host) -> Result<u32> {
    let mut modulator_1 = Modulator::new(vec![1000.0, 10000.0], 48000, false);
    modulator_1.test_carrier_wave().await;
    return Ok(0);
}

pub async fn pa1(sel: i32) -> Result<u32> {
    let available_sel = vec![0, 1, 2];
    if !available_sel.contains(&sel) {
        return Err(Error::msg("Invalid selection"));
    }

    let host = cpal::default_host();

    if sel == 0 || sel == 1 {
        println!("Objective 1 start");
        match pa0::pa0(0).await {
            Ok(_) => {}
            Err(e) => {
                println!("Error: {}", e);
            }
        }
        println!("Objective 1 end");
    }

    if sel == 0 || sel == 2 {
        println!("Objective 1 start");
        match obj_2(&host).await {
            Ok(_) => {}
            Err(e) => {
                println!("Error: {}", e);
            }
        }
        println!("Objective 1 end");
    }

    return Ok(0);
}
