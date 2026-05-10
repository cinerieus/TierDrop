# Build TierDrop
FROM docker.io/rust:1.93-bookworm AS builder
WORKDIR /app
COPY . .
RUN rustup target add x86_64-unknown-linux-gnu && make linux
RUN cp dist/tierdrop-*-linux-amd64 /tierdrop

# Get ZeroTier
FROM docker.io/debian:bookworm-slim AS zerotier
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    gnupg \
    && curl -s https://install.zerotier.com | bash

# Build Final Container
FROM docker.io/debian:bookworm-slim

# Get TierDrop from builder and adds entrypoint
COPY --from=builder /tierdrop /usr/bin/tierdrop
COPY docker/docker-entrypoint.sh /usr/bin/

# Install basic dependencies
RUN apt update && \
    apt-get install --no-install-recommends -y ca-certificates iproute2 && \
    rm -rf /var/lib/apt/lists/* && \
    chmod +x /usr/bin/docker-entrypoint.sh && \
    mkdir -p /var/lib/zerotier-one

# Get ZeroTier binaries
COPY --from=zerotier /usr/sbin/zerotier-cli /usr/sbin/zerotier-cli
COPY --from=zerotier /usr/sbin/zerotier-idtool /usr/sbin/zerotier-idtool
COPY --from=zerotier /usr/sbin/zerotier-one /usr/sbin/zerotier-one

# ZeroTier UDP port + TierDrop web UI
EXPOSE 9993/udp 8000

# Persist ZeroTier identity and TierDrop config
VOLUME ["/var/lib/zerotier-one", "/root/.local/share/tierdrop"]

ENTRYPOINT ["docker-entrypoint.sh"]
