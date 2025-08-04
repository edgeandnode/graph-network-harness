# Graph Network Harness

A heterogeneous service orchestration framework for testing Graph Protocol components and beyond.

## Overview

This framework orchestrates services across different execution environments - local processes, Docker containers, remote machines via SSH - providing unified management and network transparency. Originally built for Graph Protocol testing, the architecture is general enough for any distributed service orchestration needs.

## Architecture

The framework uses a layered architecture with clear separation of concerns. For detailed information see:

- **[Architecture Overview](docs/ARCHITECTURE.md)**: Complete system design and component relationships
- **[Current Status](docs/CURRENT-STATUS.md)**: Implementation progress and known issues
- **[Developer Guide](docs/DEVELOPER-GUIDE.md)**: How to contribute and extend the framework

### Core Components

- **[command-executor](crates/command-executor)**: Runtime-agnostic command execution across different backends
- **[service-registry](crates/service-registry)**: Service discovery and network topology management
- **[service-orchestration](crates/service-orchestration)**: Lifecycle management implementing ADR-007
- **[harness-config](crates/harness-config)**: YAML configuration parsing and resolution
- **[harness-core](crates/harness-core)**: Core abstractions for building service daemons
- **[harness](crates/harness)**: CLI for managing services via YAML configuration
- **[graph-test-daemon](crates/graph-test-daemon)**: Graph Protocol testing implementation

## Quick Start

```bash
# Install the CLI
cargo install --path crates/harness

# Create a service configuration
cat > services.yaml << 'EOF'
version: "1.0"
services:
  postgres:
    type: docker
    image: postgres:15
    env:
      POSTGRES_PASSWORD: secret
    health_check:
      command: pg_isready
      
  api:
    type: process
    binary: ./target/release/api
    env:
      DATABASE_URL: postgresql://postgres:secret@${postgres.ip}:5432/db
    dependencies:
      - postgres
EOF

# Start services
harness start

# Check status
harness status

# Stop services
harness stop
```

## Key Features

- **Multi-Backend Execution**: Local processes, Docker, SSH, systemd
- **Network Transparency**: Services communicate regardless of location
- **Dependency Management**: Automatic service startup ordering
- **Health Monitoring**: Configurable health checks
- **Event Streaming**: Real-time service output and status
- **Runtime Agnostic**: Works with any async runtime

## Graph Protocol Testing

The `graph-test-daemon` provides specialized services for Graph Protocol development with YAML-based configuration:

```bash
# Start daemon with YAML configuration (required)
graph-test-daemon --config graph-stack.yaml --endpoint 127.0.0.1:9443
```

Services available through service type linking:
- **postgres**: Database operations and queries  
- **anvil**: Blockchain mining and balance management
- **graph-node**: Subgraph deployment and indexing
- **ipfs**: Content storage and retrieval

Each service provides domain-specific actions accessible through the daemon API. See [graph-test-daemon documentation](crates/graph-test-daemon/README.md) for configuration examples.

## Documentation

- **[Architecture Overview](docs/ARCHITECTURE.md)**: System design, component relationships, and extension points
- **[Developer Guide](docs/DEVELOPER-GUIDE.md)**: Setup, common tasks, and contribution guidelines
- **[Current Status](docs/CURRENT-STATUS.md)**: What's implemented, what's missing, known issues
- **[Code Policy](docs/CODE-POLICY.md)**: Coding standards and architectural principles
- **[ADRs](ADRs/)**: Architecture Decision Records documenting key design choices

## Development

See [CI.md](CI.md) for testing and development workflows:

```bash
# Run all CI checks
cargo xtask ci all

# Run specific tests
cargo xtask test --package service-registry
```

## Current Status

Core orchestration is complete (ADR-007 ~80% implemented). The framework is functional with CLI support for YAML-based service management. Advanced features like WireGuard mesh networking and enhanced CLI UX are planned.

## License

Licensed under MIT or Apache-2.0, at your option.