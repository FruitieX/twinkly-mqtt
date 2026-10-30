#![allow(clippy::redundant_closure_call)]

use anyhow::{Context, Result};
use rand::{distr::Alphanumeric, RngExt};
use rumqttc::{AsyncClient, MqttOptions, QoS};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{
    sync::{watch::Receiver, RwLock},
    task,
};

use crate::config::MqttConfig;
use crate::twinkly::TwinklyConfig;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct MqttDevice {
    pub id: String,
    pub name: String,
    pub power: Option<bool>,
    pub brightness: Option<f32>,
    pub transition_ms: Option<f32>,
    pub sensor_value: Option<String>,
}

#[derive(Clone)]
pub struct MqttClient {
    pub client: AsyncClient,
    pub rx_map: HashMap<String, Arc<RwLock<Receiver<Option<MqttDevice>>>>>,
}

/// Builds a unique MQTT client id by appending a random suffix to `id`.
fn client_id(id: &str) -> String {
    let random_string: String = rand::rng()
        .sample_iter(Alphanumeric)
        .take(8)
        .map(char::from)
        .collect();

    format!("{id}-{random_string}")
}

pub async fn init_mqtt(
    mqtt_config: &MqttConfig,
    twinkly_config: &TwinklyConfig,
) -> Result<MqttClient> {
    let mut options = MqttOptions::new(
        client_id(&mqtt_config.id),
        mqtt_config.host.clone(),
        mqtt_config.port,
    );
    options.set_keep_alive(Duration::from_secs(5));
    let (client, mut eventloop) = AsyncClient::new(options, 10);

    let mut tx_map = HashMap::new();
    let mut rx_map = HashMap::new();

    for device in twinkly_config.devices.values() {
        let (tx, rx) = tokio::sync::watch::channel(None);
        let tx = Arc::new(RwLock::new(tx));
        let rx = Arc::new(RwLock::new(rx));
        tx_map.insert(device.id.clone(), tx);
        rx_map.insert(device.id.clone(), rx);
    }

    {
        let client = client.clone();
        task::spawn(async move {
            loop {
                let notification = eventloop.poll().await;
                let mqtt_tx = tx_map.clone();

                let client = client.clone();
                let res = (|| async move {
                    match notification? {
                        rumqttc::Event::Incoming(rumqttc::Packet::ConnAck(_)) => {
                            client
                                .subscribe("home/lights/twinkly/+/set", QoS::AtMostOnce)
                                .await?;
                        }
                        rumqttc::Event::Incoming(rumqttc::Packet::Publish(msg)) => {
                            let device: MqttDevice = serde_json::from_slice(&msg.payload)?;

                            let device_id = &device.id;
                            let tx = mqtt_tx.get(device_id).context(format!(
                                "Could not find configured MQTT device with id {}",
                                device_id
                            ))?;
                            let tx = tx.write().await;
                            tx.send(Some(device))?;
                        }
                        _ => {}
                    }

                    Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                })()
                .await;

                if let Err(e) = res {
                    eprintln!("MQTT error: {:?}", e);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        });
    }

    Ok(MqttClient { client, rx_map })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_id_has_random_suffix() {
        let a = client_id("twinkly-mqtt");
        let b = client_id("twinkly-mqtt");

        assert!(a.starts_with("twinkly-mqtt-"));
        assert_eq!(a.len(), "twinkly-mqtt-".len() + 8);
        assert!(a["twinkly-mqtt-".len()..]
            .chars()
            .all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(a, b);
    }

    #[test]
    fn device_json_round_trips() {
        let device = MqttDevice {
            id: "Staircase".to_string(),
            name: "Staircase".to_string(),
            power: Some(true),
            brightness: None,
            transition_ms: None,
            sensor_value: None,
        };

        let json = serde_json::to_string(&device).unwrap();
        let decoded: MqttDevice = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, device);
    }

    #[test]
    fn decodes_set_message() {
        let device: MqttDevice =
            serde_json::from_str(r#"{"id": "Staircase", "name": "Staircase", "power": false}"#)
                .unwrap();

        assert_eq!(device.power, Some(false));
    }
}
