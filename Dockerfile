FROM gcr.io/distroless/static@sha256:9be3fcc6abeaf985b5ecce59451acbcbb15e7be39472320c538d0d55a0834edc
COPY target/x86_64-unknown-linux-musl/release/twinkly-mqtt /usr/local/bin/twinkly-mqtt
CMD ["twinkly-mqtt"]
