#!/bin/sh
set -e

# Keep container running on error for debugging
keep_alive_on_error() {
    echo "ERROR: Container failed. Keeping container alive for debugging..."
    echo "You can inspect the container with: docker exec -it <container-id> sh"
    # Keep the container running
    tail -f /dev/null
}

# Set up error trap
trap 'keep_alive_on_error' ERR

# Create logs directory if it doesn't exist and ensure it's writable
if [ -d "/workspace/integration-tests" ]; then
    mkdir -p /workspace/integration-tests/logs
    # Make logs directory world-writable so tests can write logs
    chmod 777 /workspace/integration-tests/logs
fi

# Function to wait for Docker daemon
wait_for_docker() {
    echo "Waiting for Docker daemon to start..."
    max_attempts=30
    attempt=0
    
    while [ $attempt -lt $max_attempts ]; do
        if docker info >/dev/null 2>&1; then
            echo "Docker daemon is ready!"
            return 0
        fi
        echo "Attempt $((attempt + 1))/$max_attempts: Docker daemon not ready yet..."
        sleep 2
        attempt=$((attempt + 1))
    done
    
    echo "ERROR: Docker daemon failed to start after $max_attempts attempts"
    return 1
}

# Start Docker daemon in DinD mode for isolated container execution
echo "Starting Docker daemon in DinD mode..."

# Start Docker daemon in the background
dockerd \
    --host=unix:///var/run/docker.sock \
    --host=tcp://0.0.0.0:2375 \
    --storage-driver=overlay2 &

# Store the PID
DOCKERD_PID=$!

# Wait for Docker to be ready
if ! wait_for_docker; then
    echo "Docker daemon logs:"
    kill -0 $DOCKERD_PID 2>/dev/null && kill $DOCKERD_PID
    keep_alive_on_error
fi

# Check if host Docker socket is available for image access
if [ -S /var/run/docker-host.sock ]; then
    echo "Host Docker socket detected at /var/run/docker-host.sock"
    echo "Setting up hybrid mode: DinD for containers, host for images"
    
    # Create a helper script to pull images from host if they exist
    cat > /usr/local/bin/sync-image-from-host <<'EOF'
#!/bin/sh
IMAGE="$1"
if [ -z "$IMAGE" ]; then
    echo "Usage: sync-image-from-host <image:tag>"
    exit 1
fi

# Check if image exists on host
if docker --host unix:///var/run/docker-host.sock image inspect "$IMAGE" >/dev/null 2>&1; then
    echo "Found image on host: $IMAGE"
    echo "Transferring to DinD..."
    
    # Export from host and import to DinD
    docker --host unix:///var/run/docker-host.sock save "$IMAGE" | docker load
    echo "Image transferred successfully"
else
    echo "Image not found on host: $IMAGE"
    echo "Will pull from registry when needed"
fi
EOF
    chmod +x /usr/local/bin/sync-image-from-host
    
    # Sync commonly used base images from host if available
    echo "Syncing common images from host if available..."
    for image in alpine:latest docker:28 node:18-alpine python:3.11-alpine postgres:15-alpine; do
        sync-image-from-host "$image" || true
    done
else
    echo "No host Docker socket found. Running in pure DinD mode."
fi

# If no command provided or just "sh", keep container running
if [ $# -eq 0 ] || ([ $# -eq 1 ] && [ "$1" = "sh" ]); then
    echo "No command provided. Keeping container running..."
    echo "You can now run commands inside the container."
    echo "Docker daemon is ready at unix:///var/run/docker.sock"
    tail -f /dev/null
else
    # Execute the command passed to the container
    exec "$@"
fi