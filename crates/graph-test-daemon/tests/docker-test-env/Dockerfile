# Basic multi-process container for Graph Protocol testing
FROM ubuntu:22.04

ARG DEBIAN_FRONTEND=noninteractive

# Install essential dependencies
RUN apt-get update && apt-get install -y \
    # SSH server for remote access
    openssh-server \
    # Init system for proper process management
    systemd \
    systemd-sysv \
    # Basic tools
    sudo \
    curl \
    wget \
    git \
    build-essential \
    # Network tools for debugging
    netcat \
    iproute2 \
    iputils-ping \
    net-tools \
    # PostgreSQL
    postgresql-14 \
    postgresql-client-14 \
    # Python (useful for scripts)
    python3 \
    python3-pip \
    # Docker (for potential nested containers)
    docker.io \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js (needed by Graph Node)
RUN curl -fsSL https://deb.nodesource.com/setup_18.x | bash - && \
    apt-get install -y nodejs

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Install Foundry (includes anvil for local Ethereum)
RUN curl -L https://foundry.paradigm.xyz | bash && \
    /root/.foundry/bin/foundryup
ENV PATH="/root/.foundry/bin:${PATH}"

# Install IPFS
RUN wget https://dist.ipfs.tech/kubo/v0.24.0/kubo_v0.24.0_linux-amd64.tar.gz && \
    tar -xvzf kubo_v0.24.0_linux-amd64.tar.gz && \
    cd kubo && \
    bash install.sh && \
    rm -rf /kubo_v0.24.0_linux-amd64.tar.gz /kubo

# Create test user with sudo
RUN useradd -m -s /bin/bash testuser && \
    echo 'testuser:testpass' | chpasswd && \
    usermod -aG sudo,docker testuser && \
    echo 'testuser ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers

# Setup SSH
RUN mkdir -p /home/testuser/.ssh && \
    chown testuser:testuser /home/testuser/.ssh && \
    chmod 700 /home/testuser/.ssh && \
    mkdir -p /var/run/sshd

# Configure SSH to allow password and key auth
RUN sed -i 's/#PermitRootLogin prohibit-password/PermitRootLogin no/' /etc/ssh/sshd_config && \
    sed -i 's/#PasswordAuthentication yes/PasswordAuthentication yes/' /etc/ssh/sshd_config && \
    sed -i 's/#PubkeyAuthentication yes/PubkeyAuthentication yes/' /etc/ssh/sshd_config

# Setup PostgreSQL directories with proper permissions
RUN mkdir -p /var/lib/postgresql/data && \
    chown -R testuser:testuser /var/lib/postgresql && \
    chmod 750 /var/lib/postgresql/data

# Initialize PostgreSQL cluster as testuser
USER testuser
RUN /usr/lib/postgresql/14/bin/initdb -D /var/lib/postgresql/data && \
    echo "host all all 0.0.0.0/0 trust" >> /var/lib/postgresql/data/pg_hba.conf && \
    echo "listen_addresses = '*'" >> /var/lib/postgresql/data/postgresql.conf
USER root

# Initialize IPFS for testuser
USER testuser
RUN ipfs init && \
    ipfs config --json API.HTTPHeaders.Access-Control-Allow-Origin '["*"]' && \
    ipfs config --json API.HTTPHeaders.Access-Control-Allow-Methods '["GET", "POST"]' && \
    ipfs config Addresses.API /ip4/0.0.0.0/tcp/5001 && \
    ipfs config Addresses.Gateway /ip4/0.0.0.0/tcp/8080
USER root

# Create working directories
RUN mkdir -p /workspace /var/log/services && \
    chown -R testuser:testuser /workspace /var/log/services

# Enable SSH service
RUN systemctl enable ssh

# Remove unnecessary systemd services for container
RUN rm -f /lib/systemd/system/multi-user.target.wants/* \
    /etc/systemd/system/*.wants/* \
    /lib/systemd/system/local-fs.target.wants/* \
    /lib/systemd/system/sockets.target.wants/*udev* \
    /lib/systemd/system/sockets.target.wants/*initctl* \
    /lib/systemd/system/sysinit.target.wants/systemd-tmpfiles-setup* \
    /lib/systemd/system/systemd-update-utmp*

# Expose ports
EXPOSE 22 5432 5001 8080 8545 8000 8001 8020 8040

WORKDIR /workspace

# Use systemd as init
CMD ["/lib/systemd/systemd"]