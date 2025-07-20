#!/bin/bash
# Script to run systemd tests in container

set -e

cd "$(dirname "$0")"

echo "Building systemd test container..."
docker-compose build

echo "Starting systemd container..."
docker-compose up -d

# Wait for systemd to be ready
echo "Waiting for systemd to initialize..."
sleep 5

# Check if systemd is running
docker-compose -f tests/systemd-container/docker-compose.yaml exec systemd-test systemctl is-system-running --wait || true

echo "Creating portable service images..."
docker-compose -f tests/systemd-container/docker-compose.yaml exec systemd-test bash -c '
cd /opt/portable-services
tar -czf echo-service.tar.gz -C echo-service .
tar -czf counter-service.tar.gz -C counter-service .
'

echo "Building systemd integration test on host..."
cd ../..
cargo test --test systemd_integration --no-run

echo "Running systemd-portable tests..."
docker-compose -f tests/systemd-container/docker-compose.yaml exec systemd-test bash -c '
cd /workspace/command-executor
# Run the pre-built test binary
./target/debug/deps/systemd_integration-* --nocapture
'

echo "Cleaning up..."
docker-compose -f tests/systemd-container/docker-compose.yaml down -v

echo "Tests completed!"