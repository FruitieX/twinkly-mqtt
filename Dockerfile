FROM gcr.io/distroless/static@sha256:95ea148e8e9edd11cc7f639dc11825f38af86a14e5c7361753c741ceadef2167
COPY target/x86_64-unknown-linux-musl/release/twinkly-mqtt /usr/local/bin/twinkly-mqtt
CMD ["twinkly-mqtt"]
