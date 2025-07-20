FROM jrei/systemd-ubuntu:22.04

ARG DEBIAN_FRONTEND=noninteractive

# Install systemd-container package (which includes portablectl)
RUN apt-get update && apt-get install -y \
    systemd-container \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Create directory for portable services
RUN mkdir -p /opt/portable-services

# Copy test portable service images
COPY test-services/ /opt/portable-services/

# We don't need Rust in the container - tests will be compiled on host and mounted

# Create working directory
WORKDIR /workspace

# Volume for systemd requirements
VOLUME ["/sys/fs/cgroup", "/tmp", "/run", "/run/lock"]

# Start systemd
CMD ["/sbin/init"]