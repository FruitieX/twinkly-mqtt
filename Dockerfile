FROM gcr.io/distroless/cc
COPY target/x86_64-unknown-linux-musl/release/twinkly-mqtt  /usr/local/bin/twinkly-mqtt
CMD ["twinkly-mqtt"]
