#!/bin/bash
# Setup test container and SSH keys

set -e

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
CONTAINER_NAME="graph-test-env"
IMAGE_NAME="graph-test-daemon:test"
SSH_DIR="$SCRIPT_DIR/ssh-keys"

# Create SSH directory if it doesn't exist
mkdir -p "$SSH_DIR"

# Generate SSH key if it doesn't exist
if [ ! -f "$SSH_DIR/test_ed25519" ]; then
    echo "Generating SSH key for test environment..."
    ssh-keygen -t ed25519 -f "$SSH_DIR/test_ed25519" -N "" -C "test@graph-test-daemon"
    cp "$SSH_DIR/test_ed25519.pub" "$SSH_DIR/authorized_keys"
    chmod 600 "$SSH_DIR/test_ed25519"
    chmod 644 "$SSH_DIR/test_ed25519.pub"
    chmod 644 "$SSH_DIR/authorized_keys"
    echo "SSH keys generated successfully"
fi

# Build Docker image
echo "Building Docker image..."
docker build -t "$IMAGE_NAME" "$SCRIPT_DIR"

# Stop and remove any existing container
docker stop "$CONTAINER_NAME" 2>/dev/null || true
docker rm "$CONTAINER_NAME" 2>/dev/null || true

# Start container with systemd
echo "Starting container..."
docker run -d \
    --name "$CONTAINER_NAME" \
    --privileged \
    --cgroupns=host \
    -v /sys/fs/cgroup:/sys/fs/cgroup:rw \
    -p 2222:22 \
    -p 5432:5432 \
    -p 5001:5001 \
    -p 8080:8080 \
    -p 8545:8545 \
    -p 8000:8000 \
    -p 8001:8001 \
    -p 8020:8020 \
    -p 8040:8040 \
    -v "$SSH_DIR/authorized_keys:/home/testuser/.ssh/authorized_keys:ro" \
    "$IMAGE_NAME"

echo ""
echo "Container '$CONTAINER_NAME' is running"
echo "SSH key: $SSH_DIR/test_ed25519"
echo "SSH command: ssh -i $SSH_DIR/test_ed25519 -p 2222 testuser@localhost"
echo ""
echo "The daemon can now connect using SSH executor with:"
echo "  Host: localhost"
echo "  Port: 2222"
echo "  User: testuser"
echo "  Key: $SSH_DIR/test_ed25519"