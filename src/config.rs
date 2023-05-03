use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;

use crate::twinkly::{TwinklyConfig, TwinklyDeviceConfig};

pub type DeviceId = String;

#[derive(Deserialize, Debug)]
pub struct MqttConfig {
    pub id: String,
    pub host: String,
    pub port: u16,
}

#[derive(Deserialize, Debug)]
pub struct DeviceConfig {
    pub ip: String,
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub mqtt: MqttConfig,
    pub devices: HashMap<DeviceId, DeviceConfig>,
}

pub fn read_config_devices() -> Result<(MqttConfig, TwinklyConfig)> {
    let builder = config::Config::builder();

    let root = std::env::current_dir().unwrap();
    let sample_path = root.join("Settings.toml.example");

    let path = root.join("Settings.toml");

    if !path.exists() && std::env::var("SKIP_SAMPLE_CONFIG").is_err() {
        println!("Settings.toml not found, generating sample configuration.");
        println!("Set SKIP_SAMPLE_CONFIG environment variable to opt out of this behavior.");
        std::fs::copy(sample_path, path).unwrap();
    }

    let builder = builder.add_source(config::File::with_name("Settings"));
    let settings = builder.build()?;

    let config: Config = settings.clone().try_deserialize().context(
        "Failed to deserialize config, compare your config file to Settings.toml.example!",
    )?;

    let devices = config
        .devices
        .into_iter()
        .map(|(device_id, device)| {
            (
                device_id.clone(),
                TwinklyDeviceConfig {
                    name: device_id.clone(),
                    id: device_id,
                    ip: device.ip,
                },
            )
        })
        .collect();

    let mqtt_config = config.mqtt;
    let twinkly_config = TwinklyConfig { devices };

    Ok((mqtt_config, twinkly_config))
}
