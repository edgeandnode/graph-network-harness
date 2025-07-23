# Graph Network Harness

A heterogeneous service orchestration framework implementing distributed service management across local processes, Docker containers, and remote machines. Built on a runtime-agnostic architecture supporting any async runtime (Tokio, async-std, smol).

## 🚀 Current Status

**Phase 4 of 6 Complete** - Service lifecycle management foundation implemented with comprehensive test coverage.

### ✅ Completed Phases

1. **Phase 1: Command Execution** - Runtime-agnostic command execution framework
2. **Phase 2: Service Registry** - Service discovery with network topology detection  
3. **Phase 3: Network Configuration** - IP allocation and cross-network service resolution
4. **Phase 4: Service Lifecycle** - Orchestrator with health monitoring and package deployment

### 🔄 In Progress

**Phase 5: Configuration & CLI** - Services.yaml parsing and command-line interface

### 📋 Upcoming

**Phase 6: Production Features** - Monitoring, scaling, and advanced orchestration

## Architecture Overview

The harness implements [ADR-007](ADRs/007-distributed-service-orchestration.md) for heterogeneous service orchestration:

```
┌─────────────────────────────────────────────────────────┐
│                   Orchestrator                          │
│  ┌─────────────┐ ┌──────────────┐ ┌─────────────────┐   │
│  │   Service   │ │   Service    │ │     Package     │   │
│  │   Manager   │ │   Registry   │ │    Deployer     │   │
│  └─────────────┘ └──────────────┘ └─────────────────┘   │
└─────────────────────────────────────────────────────────┘
                           │
      ┌────────────────────┼────────────────────┐
      │                    │                    │
┌─────▼──────┐      ┌──────▼──────┐     ┌──────▼──────┐
│  Process   │      │   Docker    │     │   Remote    │
│  Executor  │      │  Executor   │     │  Executor   │
└────────────┘      └─────────────┘     └─────────────┘
      │                    │                    │
┌─────▼──────┐      ┌──────▼──────┐     ┌──────▼──────┐
│   Local    │      │  Container  │     │    SSH      │
│  Process   │      │   (Docker)  │     │  (LAN/WG)   │
└────────────┘      └─────────────┘     └─────────────┘
```

## Key Features

### 🎯 Heterogeneous Service Execution
- **Local Processes**: Direct process execution with PID tracking
- **Docker Containers**: Container lifecycle management
- **Remote SSH**: Deploy and manage services on LAN nodes
- **WireGuard Networks**: Package-based deployment over WireGuard

### 🌐 Unified Networking
- **Service Discovery**: Automatic service registration and discovery
- **Network Topology**: Detect and route between Local, LAN, and WireGuard networks
- **IP Management**: Automatic IP allocation within configured subnets
- **Cross-Network Resolution**: Services communicate regardless of location

### 🏥 Health Monitoring
- **Configurable Health Checks**: HTTP endpoints, commands, or custom checks
- **Retry Logic**: Configure retries before marking services unhealthy
- **Continuous Monitoring**: Background health monitoring with configurable intervals

### 📦 Package Deployment
- **Remote Deployment**: Deploy service packages to remote hosts
- **Dependency Resolution**: Automatic IP injection for service dependencies
- **Version Management**: Track deployed package versions

### ⚡ Runtime Agnostic
- **Any Async Runtime**: Works with Tokio, async-std, smol, or custom runtimes
- **No Runtime Lock-in**: Libraries use async-process, async-fs, async-net
- **Flexible Integration**: Embed in any async application

## Getting Started

### Current Capabilities

While the CLI is still in development (Phase 5), you can use the orchestrator library directly:

```rust
use orchestrator::{
    ServiceManager, ServiceConfig, ServiceTarget, HealthCheck
};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create service manager
    let manager = ServiceManager::new().await?;
    
    // Define a local process service
    let service = ServiceConfig {
        name: "my-api".to_string(),
        target: ServiceTarget::Process {
            binary: "./target/release/api-server".to_string(),
            args: vec!["--port".to_string(), "8080".to_string()],
            env: HashMap::from([
                ("LOG_LEVEL".to_string(), "info".to_string()),
            ]),
            working_dir: None,
        },
        dependencies: vec!["database".to_string()],
        health_check: Some(HealthCheck {
            command: "curl".to_string(),
            args: vec!["-f".to_string(), "http://localhost:8080/health".to_string()],
            interval: 30,
            retries: 3,
            timeout: 10,
        }),
    };
    
    // Start the service
    manager.start_service("my-api", service).await?;
    
    // Check service status
    let status = manager.get_service_status("my-api").await?;
    println!("Service status: {:?}", status);
    
    Ok(())
}
```

### Docker Service Example

```rust
let docker_service = ServiceConfig {
    name: "postgres".to_string(),
    target: ServiceTarget::Docker {
        image: "postgres:15".to_string(),
        env: HashMap::from([
            ("POSTGRES_PASSWORD".to_string(), "secret".to_string()),
        ]),
        ports: vec![5432],
        volumes: vec!["/data/postgres:/var/lib/postgresql/data".to_string()],
    },
    dependencies: vec![],
    health_check: Some(HealthCheck {
        command: "pg_isready".to_string(),
        args: vec![],
        interval: 10,
        retries: 5,
        timeout: 5,
    }),
};
```

## Project Structure

```
graph-network-harness/
├── crates/
│   ├── command-executor/     # Runtime-agnostic command execution
│   ├── service-registry/     # Service discovery & network topology
│   └── orchestrator/         # Service lifecycle orchestration
├── ADRs/                     # Architecture Decision Records
│   ├── 007-distributed-service-orchestration.md
│   └── 007-implementation/
│       └── plan.md          # Implementation phases
├── docker-test-env/         # Docker-in-Docker test environment
└── test-activity/           # Test logs and artifacts
```

## Development

### Prerequisites

- Rust toolchain (1.70+)
- Docker (for container execution)
- SSH access (for remote execution)

### Building

```bash
# Build all crates
cargo build --workspace

# Run tests
cargo test --workspace

# Run with specific features
cargo test -p service-registry --features docker-tests
```

### Testing Philosophy

**TEST EARLY AND OFTEN**: Each phase is validated by comprehensive tests before moving forward.

- Unit tests for all components
- Integration tests with real Docker containers
- Runtime-agnostic tests using smol
- 30+ tests in orchestrator alone

## Roadmap

### Phase 5: Configuration & CLI (Next)
- [ ] Parse services.yaml configuration files
- [ ] CLI commands: start, stop, status, deploy, logs
- [ ] Service dependency resolution
- [ ] Network topology detection

### Phase 6: Production Features
- [ ] Distributed tracing integration
- [ ] Prometheus metrics export
- [ ] Service scaling and load balancing
- [ ] Advanced health check strategies
- [ ] Rolling updates and rollbacks

## Design Principles

1. **Runtime Agnostic**: No hard dependency on any specific async runtime
2. **Composable**: Each crate can be used independently
3. **Testable**: Comprehensive test coverage with real infrastructure
4. **Extensible**: Easy to add new executor types or features
5. **Production Ready**: Built for reliability and observability

## Contributing

See [CLAUDE.md](CLAUDE.md) for AI assistant guidelines and [CODE-POLICY.md](CODE-POLICY.md) for coding standards.

Key points:
- Use runtime-agnostic async libraries
- Follow error composition patterns with thiserror
- Write tests alongside implementation
- Document all public APIs

## License

[License details here]

## Acknowledgments

Built as a modern replacement for Docker-only orchestration, enabling true heterogeneous service deployment across any infrastructure.
