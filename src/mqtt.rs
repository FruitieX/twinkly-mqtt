#![allow(clippy::redundant_closure_call)]

use anyhow::{Context, Result};
use rand::{distributions::Alphanumeric, Rng};
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

pub async fn init_mqtt(
    mqtt_config: &MqttConfig,
    twinkly_config: &TwinklyConfig,
) -> Result<MqttClient> {
    let random_string: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect();

    let mut options = MqttOptions::new(
        format!("{}-{}", mqtt_config.id.clone(), random_string),
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
