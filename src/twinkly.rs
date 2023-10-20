#![allow(clippy::redundant_closure_call)]

use anyhow::{Context, Result};
use futures::{future::select_all, FutureExt};
use log::warn;
use rumqttc::QoS;
use serde::Deserialize;
use std::time::Duration;
use std::{collections::HashMap, sync::Arc};
use tokio::task;
use tokio::{sync::RwLock, time::timeout};

use crate::api::TwinklyApi;
use crate::mqtt::{MqttClient, MqttDevice};

#[derive(Clone, Debug, Deserialize)]
pub struct TwinklyDeviceConfig {
    pub name: String,
    pub id: String,
    pub ip: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TwinklyConfig {
    pub devices: HashMap<String, TwinklyDeviceConfig>,
}

pub async fn init_twinkly(
    device_config: TwinklyDeviceConfig,
    mqtt_client: MqttClient,
) -> Result<()> {
    task::spawn(async move {
        loop {
            let twinkly = Arc::new(RwLock::new(TwinklyApi::new(device_config.ip.clone())));
            let mqtt_client = mqtt_client.clone();

            // Loop until there's an error of any kind
            let poll_future = {
                let twinkly = twinkly.clone();
                let device_config = device_config.clone();

                (|| async move {
                    loop {
                        let mode = {
                            let twinkly = twinkly.write().await;
                            timeout(Duration::from_millis(5000), twinkly.get_mode()).await??
                        };

                        let power = &mode != "off";

                        let mqtt_device = MqttDevice {
                            id: device_config.id.clone(),
                            name: device_config.id.clone(),
                            power: Some(power),
                            brightness: None,
                            transition_ms: None,
                            sensor_value: None,
                        };

                        let json = serde_json::to_string(&mqtt_device)?;

                        let topic = format!("home/lights/twinkly/{}", device_config.id);
                        mqtt_client
                            .client
                            .publish(topic, QoS::AtLeastOnce, true, json)
                            .await?;

                        // restrict polling rate if phone app is being used
                        if mode == "rt" {
                            tokio::time::sleep(Duration::from_millis(60000)).await;
                        } else {
                            tokio::time::sleep(Duration::from_millis(3000)).await;
                        }
                    }

                    #[allow(unreachable_code)]
                    Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                })()
            };

            let send_future = {
                let mqtt_rx_map = mqtt_client.rx_map.clone();
                let device_config = device_config.clone();

                (|| async move {
                    loop {
                        let res = {
                            let mqtt_rx = mqtt_rx_map.get(&device_config.id).context(format!(
                                "Could not find configured MQTT device with id {}",
                                device_config.id
                            ))?;
                            let mut mqtt_rx = mqtt_rx.write().await;
                            mqtt_rx.changed().await?;
                            let value = &*mqtt_rx.borrow();
                            value
                                .clone()
                                .context("Expected to receive mqtt message from rx channel")?
                        };

                        let twinkly = twinkly.write().await;
                        let original_mode =
                            timeout(Duration::from_millis(3000), twinkly.get_mode()).await??;

                        if res.power == Some(true) && original_mode == "off" {
                            // Device is off and it is requested to be powered on
                            timeout(
                                Duration::from_millis(3000),
                                twinkly.set_mode("movie".to_string()),
                            )
                            .await??;
                        } else if res.power == Some(false) && original_mode != "off" {
                            // Device is on and it is requested to be powered off
                            timeout(
                                Duration::from_millis(3000),
                                twinkly.set_mode("off".to_string()),
                            )
                            .await??;
                        }
                    }

                    #[allow(unreachable_code)]
                    Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                })()
            };

            let res = select_all(vec![poll_future.boxed(), send_future.boxed()]).await;

            if let (Err(e), future_index, _) = res {
                if future_index == 0 {
                    warn!(
                        "Error while polling Twinkly device {}: {:?}",
                        device_config.name, e
                    )
                } else {
                    warn!(
                        "Error while sending to Twinkly device {}: {:?}",
                        device_config.name, e
                    )
                }
            }

            // Wait before reconnecting
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
    });

    Ok(())
}
