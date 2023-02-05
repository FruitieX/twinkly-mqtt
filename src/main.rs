use twinkly::init_twinkly;

use crate::config::read_config_devices;
use crate::mqtt::init_mqtt;

mod api;
mod config;
mod mqtt;
mod twinkly;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let (mqtt_config, twinkly_config) = read_config_devices()?;
    let mqtt_client = init_mqtt(&mqtt_config, &twinkly_config).await?;

    for device in twinkly_config.devices.into_values() {
        let mqtt_client = mqtt_client.clone();
        init_twinkly(device, mqtt_client).await?;
    }

    tokio::signal::ctrl_c().await?;

    Ok(())
}
