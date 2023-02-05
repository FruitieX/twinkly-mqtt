# twinkly-mqtt

This program periodically polls configured Twinkly devices in your local network, and publishes current state on an MQTT topic: `/home/twinkly/<device_id>`.

It also subscribes to an MQTT topic `/home/twinkly/<device_id>/set` and sets device state according to incoming messages.

## Running

Make sure you have a recent version of Rust installed.

1. Clone this repo
2. Copy Settings.toml.example -> Settings.toml
3. Configure Settings.toml to match your setup (see below)
4. `cargo run`

## Configuration

For each device, you will need to retrieve and note down:

- Device local IP address
- Device name (does not have to match with Twinkly app)

You can find each device's IP in the Twinkly app.

## MQTT protocol

MQTT messages use the following JSON format:

```
{
  "id": "<device_id>",
  "name": "Staircase",
  "power": true,
  "brightness": 0.5,
}
```

If both `color` and `cct` are provided in a `/set` message, the `color` parameter will be used.

If both `brightness` and `value` are provided then the final brightness is
computed by multiplying these together. I suggest always setting `value` to 1
and adjusting `brightness` instead.