FROM rust:1.93-bookworm AS builder
WORKDIR /app
COPY . .
RUN rustup target add x86_64-unknown-linux-gnu && make linux
RUN cp dist/tierdrop-*-linux-amd64 /tierdrop

FROM debian:bookworm-slim

# Install ZeroTier and dependencies
# Note: The install script may briefly start zerotier and create identity files,
# so we explicitly remove them to ensure fresh identity on each container start
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    gnupg \
    && curl -s https://install.zerotier.com | bash \
    && rm -rf /var/lib/apt/lists/* \
    && rm -rf /var/lib/zerotier-one/*

COPY --from=builder /tierdrop /usr/local/bin/tierdrop
COPY docker/docker-entrypoint.sh /usr/local/bin/

RUN chmod +x /usr/local/bin/docker-entrypoint.sh

# ZeroTier UDP port + TierDrop web UI
EXPOSE 9993/udp 8000

# Persist ZeroTier identity and TierDrop config
VOLUME ["/var/lib/zerotier-one", "/root/.local/share/tierdrop"]

ENTRYPOINT ["docker-entrypoint.sh"]
