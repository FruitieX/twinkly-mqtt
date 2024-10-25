FROM gcr.io/distroless/static@sha256:cc226ca14d17d01d4b278d9489da930a0dd11150df10ae95829d13e6d00fbdbf
COPY target/x86_64-unknown-linux-musl/release/twinkly-mqtt /usr/local/bin/twinkly-mqtt
CMD ["twinkly-mqtt"]
