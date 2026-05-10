# syntax=docker/dockerfile:1.7
# Glama listing only — clipboard capture requires native macOS/Linux host.
# See docs/GLAMA-DOCKERFILE-NOTES.md.

FROM --platform=$BUILDPLATFORM rust:1.95-bookworm AS builder
WORKDIR /src
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libxcb1-dev libxcb-render0-dev libxcb-shape0-dev \
    libxcb-xfixes0-dev libdbus-1-dev \
 && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked --bin clipboard-history-mcp

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    libxcb1 libdbus-1-3 ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd -m -u 1000 mcp
COPY --from=builder /src/target/release/clipboard-history-mcp /usr/local/bin/
ENV CLIPBOARD_DATA_DIR=/tmp/clipboard \
    CLIPBOARD_EPHEMERAL_KEY=1 \
    RUST_LOG=info
USER mcp
RUN mkdir -p /tmp/clipboard
ENTRYPOINT ["/usr/local/bin/clipboard-history-mcp", "serve"]
