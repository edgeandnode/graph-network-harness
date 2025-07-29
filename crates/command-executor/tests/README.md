# Command Executor Tests

This directory contains tests for the command-executor crate.

## Test Organization

### Unit Tests
Basic functionality tests that don't require external dependencies:
- `local_executor.rs` - Basic command execution
- `nested_launchers.rs` - Type composition tests
- `process_cleanup.rs` - Process lifecycle management
- `error_context.rs` - Error propagation

### Integration Tests
Tests requiring external services, controlled by feature flags:

#### SSH Tests (`ssh-tests` feature)
- `integration_nested.rs` - SSH launcher integration
- `nested_attachers.rs` - SSH attacher functionality

#### Docker Tests (`docker-tests` feature)
- `ssh_docker.rs` - SSH with Docker targets

#### Systemd Tests
- `systemd_integration.rs` - Runs inside systemd container
- `systemd_portable_ssh.rs` - Tests systemd via SSH

## Running Tests

### Basic Tests (No External Dependencies)
```bash
cargo test -p command-executor
```

### SSH Tests
Requires SSH access to localhost:
```bash
cargo test -p command-executor --features ssh-tests
```

### Docker Tests
Requires Docker installed and running:
```bash
cargo test -p command-executor --features docker-tests
```

### Systemd Tests

#### Option 1: Inside Container
```bash
cd tests/systemd-container
./run-systemd-tests.sh
```

#### Option 2: Via SSH
```bash
# Start the SSH-enabled systemd container
cd tests/systemd-container
./run-ssh-tests.sh start

# Run tests
cd ../..
cargo test -p command-executor --features ssh-tests,docker-tests

# Stop container
cd tests/systemd-container
./run-ssh-tests.sh stop
```

### All Integration Tests
```bash
cargo test -p command-executor --features integration-tests,ssh-tests,docker-tests
```

## Test Infrastructure

### systemd-container/
Contains Docker setup for testing systemd functionality:
- `Dockerfile` - Basic systemd container
- `Dockerfile.ssh` - SSH-enabled systemd container
- `docker-compose.yaml` - Basic container orchestration
- `docker-compose-ssh.yaml` - SSH container orchestration
- `test-services/` - Sample portable services for testing

### common/
Shared test utilities:
- `mod.rs` - Common test helpers and configurations
