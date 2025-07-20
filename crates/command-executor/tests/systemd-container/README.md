# Systemd Container for Testing

This directory contains a Docker container setup for testing systemd-portable functionality without requiring sudo privileges on the host system.

## Overview

The container runs Ubuntu 24.04 with systemd as PID 1, allowing us to test real `portablectl` commands and systemd service management.

## Structure

- `Dockerfile` - Ubuntu 24.04 with systemd and systemd-container packages
- `docker-compose.yaml` - Container configuration with required privileges
- `create-test-services.sh` - Creates test portable service images
- `run-systemd-tests.sh` - Automated test runner
- `test-services/` - Test portable service definitions

## Usage

### Quick Test Run

```bash
# Run all systemd integration tests
./run-systemd-tests.sh
```

### Manual Testing

```bash
# Start the container
docker-compose up -d

# Enter the container
docker-compose exec systemd-test bash

# Inside container - create portable images
cd /opt/portable-services
tar -czf echo-service.tar.gz -C echo-service .
tar -czf counter-service.tar.gz -C counter-service .

# Test portablectl commands
portablectl list
portablectl attach --copy=copy /opt/portable-services/echo-service.tar.gz
systemctl start echo-service.service
journalctl -u echo-service.service -f

# Run tests
cd /workspace/command-executor
cargo test --test systemd_integration

# Clean up
docker-compose down -v
```

## Test Services

### echo-service
A simple service that prints timestamps every 5 seconds.

### counter-service
A service that maintains a counter, incrementing every 2 seconds and persisting the value.

## Requirements

- Docker with support for privileged containers
- Docker Compose
- Linux host (systemd containers don't work well on macOS/Windows)

## How It Works

1. The container runs with `--privileged` flag to allow systemd to manage cgroups
2. `/sbin/init` is the entrypoint, starting systemd as PID 1
3. Test portable services are created as tar.gz archives
4. Integration tests use real `portablectl` and `systemctl` commands
5. No sudo required on the host system

## Troubleshooting

### Container won't start
- Ensure Docker daemon supports privileged containers
- Check that cgroup v2 is available on your system

### Systemd not running
- Wait a few seconds after container start for systemd to initialize
- Check logs: `docker-compose logs systemd-test`

### Tests fail
- Ensure portable service images are created in `/opt/portable-services/`
- Check systemd status: `docker-compose exec systemd-test systemctl status`