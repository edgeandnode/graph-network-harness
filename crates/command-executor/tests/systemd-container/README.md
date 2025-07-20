# Systemd Container for Testing

This directory contains Docker infrastructure for testing systemd-portable functionality and SSH remote execution.

## Quick Start

```bash
# Generate SSH keys if needed
./generate-ssh-keys.sh

# Run tests (container will be started automatically)
cd ../..
cargo test -p command-executor --features ssh-tests,docker-tests
```

## Overview

The container runs Ubuntu with systemd as PID 1, allowing us to test:
- Real `portablectl` commands and systemd service management
- SSH remote execution with key-based authentication
- Nested launcher functionality (SSH + systemd)

## Infrastructure

### Core Files
- `Dockerfile` - Systemd container with SSH support
- `docker-compose.yaml` - Container configuration
- `generate-ssh-keys.sh` - Generates test SSH keys

### Test Resources
- `test-services/` - Test portable service fixtures (checked in)
  - `echo-service` - Prints timestamps every 5 seconds
  - `counter-service` - Maintains and increments a counter
- `ssh-keys/` - Generated SSH keys (gitignored)

## Container Management

The test infrastructure uses the command-executor library itself to manage containers, demonstrating dogfooding. A shared container is automatically started when tests run and cleaned up when tests complete.

### Manual Container Management

```bash
# Start container manually
docker-compose up -d

# Check status
docker ps -f name=command-executor-systemd-ssh-test

# View logs
docker-compose logs

# Stop container
docker-compose down

# Connect via SSH
ssh -i ssh-keys/test_ed25519 -p 2223 testuser@localhost
```

## SSH Key Management

SSH keys are automatically generated and managed:
- Keys are stored in `ssh-keys/` (gitignored)
- ED25519 and RSA keys are generated for compatibility
- Keys are automatically mounted into the container
- No passwords required for testing

## Test Development

When writing tests that need SSH:

```rust
use command_executor::{Executor, Target, Command};
use command_executor::backends::ssh::SshLauncher;

// Import shared container management
mod common;
use common::shared_container::{ensure_container_running, get_ssh_config};

// Ensure container is running
ensure_container_running().await?;

// Create SSH launcher
let local = LocalLauncher;
let ssh_launcher = SshLauncher::new(local, get_ssh_config());
let executor = Executor::new("my-test".to_string(), ssh_launcher);

// Execute commands
let cmd = Command::builder("systemctl").arg("status").build();
let result = executor.execute(&Target::Command, cmd).await?;
```

## Requirements

- Docker with support for privileged containers
- Docker Compose
- Linux host (systemd containers work best on Linux)
- Port 2223 available for SSH

## Troubleshooting

### SSH Connection Fails
- Check if container is running: `docker ps`
- Verify SSH keys exist: `ls ssh-keys/`
- View logs: `docker-compose logs`

### Systemd Issues
- Systemd may show as "degraded" in containers (normal)
- Wait a few seconds after container start
- Check systemd status via SSH:
  ```bash
  ssh -i ssh-keys/test_ed25519 -p 2223 testuser@localhost systemctl status
  ```

### Test Failures
- Ensure container is running: `docker ps -f name=command-executor-systemd-ssh-test`
- Check if test-services are mounted properly
- Verify SSH connectivity with manual SSH command
- Generate SSH keys if missing: `./generate-ssh-keys.sh`

## Security Note

The SSH keys and container configuration are for testing only:
- Keys are gitignored and should never be committed
- Container has permissive sudo rules
- SSH is configured for ease of testing, not security