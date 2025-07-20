# Systemd container with SSH support using Ubuntu 22.04
FROM ubuntu:22.04

ARG DEBIAN_FRONTEND=noninteractive

# Install systemd and SSH
RUN apt-get update && apt-get install -y \
    systemd \
    systemd-sysv \
    systemd-container \
    openssh-server \
    sudo \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Remove unnecessary systemd services
RUN rm -f /lib/systemd/system/multi-user.target.wants/* \
    /etc/systemd/system/*.wants/* \
    /lib/systemd/system/local-fs.target.wants/* \
    /lib/systemd/system/sockets.target.wants/*udev* \
    /lib/systemd/system/sockets.target.wants/*initctl* \
    /lib/systemd/system/sysinit.target.wants/systemd-tmpfiles-setup* \
    /lib/systemd/system/systemd-update-utmp*

# Enable SSH service
RUN systemctl enable ssh

# Create test user with sudo access
RUN useradd -m -s /bin/bash testuser && \
    echo 'testuser:testpass' | chpasswd && \
    usermod -aG sudo testuser && \
    echo 'testuser ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers

# Create SSH directory for testuser
RUN mkdir -p /home/testuser/.ssh && \
    chown testuser:testuser /home/testuser/.ssh && \
    chmod 700 /home/testuser/.ssh

# Configure SSH for key-based authentication
RUN sed -i 's/#PubkeyAuthentication yes/PubkeyAuthentication yes/' /etc/ssh/sshd_config && \
    sed -i 's/#PasswordAuthentication yes/PasswordAuthentication no/' /etc/ssh/sshd_config && \
    sed -i 's/#PermitRootLogin prohibit-password/PermitRootLogin no/' /etc/ssh/sshd_config && \
    sed -i 's/#StrictModes yes/StrictModes no/' /etc/ssh/sshd_config

# Create directory for portable services
RUN mkdir -p /opt/portable-services

# Copy test portable service images
COPY test-services/ /opt/portable-services/

# Create working directory
WORKDIR /workspace

# Expose SSH port
EXPOSE 22

# Use systemd as init
CMD ["/lib/systemd/systemd"]