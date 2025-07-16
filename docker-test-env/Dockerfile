# Docker-in-Docker container for integration tests with host image access
FROM docker:28-dind

# Install required tools
RUN apk add --no-cache \
    bash \
    curl \
    git \
    make \
    gcc \
    musl-dev \
    openssl-dev \
    pkgconfig \
    python3 \
    py3-pip \
    cargo \
    rust

# Install docker-compose
RUN apk add --no-cache docker-cli-compose

# Install additional tools for debugging
RUN apk add --no-cache \
    jq \
    vim \
    less \
    htop \
    net-tools

# Create workspace directory
WORKDIR /workspace

# Copy entrypoint script
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# Set up Docker socket permissions
RUN mkdir -p /var/run

# Volume for workspace
VOLUME /workspace

# Note: No ports are exposed to the host to maintain isolation
# Services run inside the DinD container's own network namespace

ENTRYPOINT ["/entrypoint.sh"]
CMD ["sh"]