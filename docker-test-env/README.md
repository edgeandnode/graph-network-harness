# Docker Test Environment

This directory contains a hybrid Docker-in-Docker (DinD) setup for running integration tests with complete container isolation while reusing host Docker images.

## Architecture

The setup provides:
- **Docker-in-Docker (DinD)**: Isolated Docker daemon for container execution
- **Host Image Access**: Ability to sync images from host Docker to avoid rebuilds
- **Container Isolation**: Prevents container name conflicts with host Docker
- **Log Persistence**: Direct volume mount for easy log access

## Why Use This Environment?

The local-network docker-compose setup uses fixed container names (e.g., `container_name: postgres`), which prevents multiple test runs from executing simultaneously. By running tests inside a DinD container, we achieve:

1. **Complete isolation**: Each test run gets its own Docker environment
2. **No conflicts**: Multiple test runs can execute in parallel without name collisions
3. **Clean state**: Each run starts with a fresh Docker environment
4. **Portability**: Tests run identically on any machine with Docker

## Usage

### Quick Start

```bash
# Navigate to parent directory
cd ../

# Run tests with automatic image sync (recommended for first run)
cargo run --bin stack-runner -- container --sync-images --local-network ../local-network

# Run tests without image sync (faster if images already synced)
cargo run --bin stack-runner -- container --local-network ../local-network

# Run specific commands
cargo run --bin stack-runner -- logs --summary
cargo run --bin stack-runner -- container --local-network ../local-network all
```

### Interactive Shell

```bash
# Start the container and get a shell
docker-compose up -d
docker-compose exec integration-tests-dind sh

# Inside the container:
cd /workspace/local-network-harness
cargo run --bin stack-runner -- harness --local-network ../local-network
```

### Direct Usage from Host

```bash
# The harness manages the container automatically
cd ../
cargo run --bin stack-runner -- harness --keep-running --local-network ../local-network
```

## Key Features

### 1. Hybrid Mode
The container runs its own Docker daemon (DinD) but can access host Docker images through a read-only socket mount at `/var/run/docker-host.sock`. This provides isolation while allowing image reuse.

### 2. Image Synchronization
The `--sync-images` flag triggers automatic synchronization of docker-compose images from host to DinD:
- Checks if each image exists on host
- Transfers to DinD using docker save/load
- Skips images not found on host (will be pulled in DinD)
- Now handled automatically by the stack-runner harness

### 3. Build Cache
Cargo cache is persisted across runs in named volumes for faster builds.

### 4. Log Access
Integration test logs are directly mounted to the host at `../test-activity/` for immediate access.

## Architecture Details

```
Host Machine
└── Docker
    └── DinD Container (integration-tests-dind)
        ├── Docker Daemon (dockerd)
        ├── Rust/Cargo environment
        ├── Integration test binary
        ├── Host Docker socket (read-only at /var/run/docker-host.sock)
        └── Docker Containers (managed by harness)
            ├── chain (Anvil)
            ├── postgres
            ├── ipfs
            ├── graph-node
            └── ... (other services)
```

## Volumes and Mounts

- `/workspace`: Your project directory (mounted from host)
- `/var/lib/docker`: Docker data for DinD (persisted in `dind-docker-data` volume)
- `/var/run/docker-host.sock`: Host Docker socket (read-only, for image access)
- `/usr/local/cargo/registry`: Cargo registry cache (persisted in `cargo-cache` volume)
- `/usr/local/cargo/git`: Cargo git cache (persisted in `cargo-git` volume)
- `/workspace/test-activity`: Test logs and artifacts (mounted directly to host)

## Network Isolation

Services run inside the DinD container have their own network namespace. No port mappings are needed to the host, which prevents conflicts with services running on the host machine.

## Files

- `docker-compose.yaml`: Container definition for DinD environment
- `Dockerfile`: Custom DinD container with Rust toolchain
- `entrypoint.sh`: Container startup script
- `README.md`: This documentation

## Container Management

```bash
# View container status
docker-compose ps

# View container logs
docker-compose logs -f

# Stop and remove the container
docker-compose down

# Clean up everything including volumes (warning: removes build caches)
docker-compose down -v
```

## Log Management

Test logs are written to:
- Session logs: `../test-activity/logs/`

The logs are mounted from the host, so you can view them in real-time as tests run.

## Troubleshooting

### Container won't start
- Check for conflicting containers: `docker ps -a | grep integration-tests-dind`
- Ensure Docker Desktop/daemon is running
- Check Docker logs: `docker-compose logs`

### Builds are slow
- Use `--sync-images` to reuse host images
- Check if cargo cache volumes exist: `docker volume ls | grep cargo`
- Ensure the container has enough resources allocated

### Permission issues
- The container runs as root internally
- Log directories are created with appropriate permissions
- If you see permission errors, check the entrypoint.sh script

### Docker daemon issues
- The container needs privileged mode for DinD
- Check if the daemon started: `docker exec integration-tests-dind docker version`
- View daemon logs in container output

## Performance Tips

1. **First Run**: Use `--sync-images` to transfer existing images from host
2. **Subsequent Runs**: Images are cached in the DinD volume
3. **Development**: Keep the container running and exec into it for faster iteration
4. **CI/CD**: Can run multiple isolated instances in parallel