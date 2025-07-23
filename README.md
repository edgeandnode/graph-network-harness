# Graph Network Harness

A heterogeneous service orchestration framework implementing distributed service management across local processes, Docker containers, and remote machines. Built on a runtime-agnostic architecture supporting any async runtime (Tokio, async-std, smol).

## 🚀 Current Status

**Phase 4 of 6 Complete** - Service lifecycle management foundation implemented with comprehensive test coverage.

### ✅ What's Working Today

- **Command Execution**: Run commands locally, via SSH, or in Docker containers
- **Service Registry**: Automatic service discovery with WebSocket-based updates
- **Network Detection**: Identify Local, LAN, and WireGuard network topologies
- **Service Orchestration**: Start/stop services with dependency management
- **Health Monitoring**: Configurable health checks with retry logic
- **Package Deployment**: Deploy service packages to remote hosts
- **IP Management**: Automatic IP allocation within configured subnets

### 🚧 What's Coming Soon

- **YAML Configuration**: Define entire service topologies in YAML (Phase 5)
- **CLI Interface**: `harness start`, `harness deploy`, etc. (Phase 5)
- **Service Scaling**: Auto-scaling based on load (Phase 6)
- **Monitoring Integration**: Prometheus/Grafana dashboards (Phase 6)
- **Rolling Updates**: Zero-downtime deployments (Phase 6)

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

## Network Testing Use Cases

### Testing Distributed Systems Across Networks

The harness excels at testing distributed systems that span multiple network boundaries. Here's how you would test a Graph Protocol indexer setup across different networks:

```yaml
# services.yaml (coming in Phase 5)
version: "1.0"

networks:
  local:
    type: local
    subnet: "127.0.0.0/8"
  
  lan:
    type: lan
    subnet: "192.168.1.0/24"
    nodes:
      - host: "192.168.1.100"
        ssh_user: "ubuntu"
        ssh_key: "~/.ssh/id_rsa"
  
  wireguard:
    type: wireguard
    subnet: "10.0.0.0/24"
    nodes:
      - host: "10.0.0.10"
        ssh_user: "admin"
        package_deploy: true

services:
  postgres:
    type: docker
    network: local
    image: "postgres:15"
    env:
      POSTGRES_PASSWORD: "${POSTGRES_PASSWORD}"
    ports:
      - 5432
    health_check:
      command: "pg_isready"
      interval: 10
      retries: 5

  graph-node:
    type: process
    network: lan
    binary: "/opt/graph-node/bin/graph-node"
    env:
      GRAPH_NODE_CONFIG: "/etc/graph-node/config.toml"
      DATABASE_URL: "postgresql://postgres:${POSTGRES_PASSWORD}@${postgres.ip}:5432/graph"
    dependencies:
      - postgres
    health_check:
      http: "http://localhost:8000/health"
      interval: 30
      retries: 3

  indexer-agent:
    type: package
    network: wireguard
    package: "./packages/indexer-agent.tar.gz"
    env:
      GRAPH_NODE_QUERY_URL: "http://${graph-node.ip}:8000"
      INDEXER_AGENT_PORT: "8080"
    dependencies:
      - graph-node
    health_check:
      http: "http://localhost:8080/health"
      timeout: 60
```

### Current Network Testing (Using Library)

Today, you can programmatically set up network tests:

```rust
use orchestrator::{ServiceManager, ServiceConfig, ServiceTarget};
use service_registry::{NetworkType, ServiceInfo};

#[tokio::test]
async fn test_cross_network_communication() {
    let manager = ServiceManager::new().await.unwrap();
    
    // Start a local database
    let db_config = ServiceConfig {
        name: "test-db".to_string(),
        target: ServiceTarget::Docker {
            image: "postgres:15".to_string(),
            env: [("POSTGRES_PASSWORD".to_string(), "test".to_string())].into(),
            ports: vec![5432],
            volumes: vec![],
        },
        dependencies: vec![],
        health_check: Some(HealthCheck {
            command: "pg_isready".to_string(),
            args: vec![],
            interval: 5,
            retries: 3,
            timeout: 5,
        }),
    };
    
    // Start service and wait for health
    manager.start_service("test-db", db_config).await.unwrap();
    
    // Get assigned IP for dependency injection
    let db_ip = manager.get_service_ip("test-db").await.unwrap();
    
    // Start a remote service that connects to the database
    let app_config = ServiceConfig {
        name: "test-app".to_string(),
        target: ServiceTarget::Remote {
            host: "192.168.1.100".to_string(),
            ssh_user: "ubuntu".to_string(),
            ssh_key: Some("/home/user/.ssh/id_rsa".to_string()),
            binary: "/opt/app/bin/server".to_string(),
            args: vec![],
            env: [
                ("DATABASE_URL".to_string(), 
                 format!("postgresql://postgres:test@{}:5432/app", db_ip))
            ].into(),
            working_dir: None,
        },
        dependencies: vec!["test-db".to_string()],
        health_check: Some(HealthCheck {
            command: "curl".to_string(),
            args: vec!["-f".to_string(), "http://localhost:8080/health".to_string()],
            interval: 10,
            retries: 3,
            timeout: 10,
        }),
    };
    
    manager.start_service("test-app", app_config).await.unwrap();
    
    // Verify cross-network connectivity
    let app_status = manager.get_service_status("test-app").await.unwrap();
    assert!(app_status.healthy);
}
```

## Getting Started

### Current Capabilities

While the YAML configuration and CLI are still in development (Phase 5), you can use the orchestrator library directly:

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

## Implementation Status

### ✅ Fully Implemented

#### Command Execution Layer
- Local process execution with PID tracking
- SSH command execution with key/password auth
- Docker container lifecycle management
- Runtime-agnostic design (works with any async runtime)

#### Service Registry
- WebSocket-based service discovery
- Network topology detection (Local/LAN/WireGuard)
- Automatic IP allocation within subnets
- Real-time service status updates

#### Orchestrator Core
- Service lifecycle management (start/stop/restart)
- Dependency resolution and ordering
- Health check monitoring with configurable retries
- Package deployment to remote hosts
- Cross-network service communication

### 🚧 Partially Implemented

#### Network Configuration
- ✅ Basic network detection and routing
- ✅ IP allocation for services
- ❌ Advanced routing rules
- ❌ Network isolation policies

#### Package Deployment
- ✅ Basic tar.gz package deployment
- ✅ Environment variable injection
- ❌ Package versioning and rollback
- ❌ Binary dependency resolution

### ❌ Not Yet Implemented

#### Configuration System (Phase 5)
- YAML service definition parsing
- Environment variable substitution
- Secret management integration
- Configuration validation

#### CLI Interface (Phase 5)
- `harness init` - Initialize project
- `harness start <service>` - Start services
- `harness stop <service>` - Stop services
- `harness status` - Show service status
- `harness logs <service>` - Stream logs
- `harness deploy` - Deploy packages

#### Production Features (Phase 6)
- Prometheus metrics export
- Distributed tracing (OpenTelemetry)
- Service auto-scaling
- Rolling updates with health checks
- Circuit breakers and retries
- Multi-region deployment

## Roadmap

### Phase 5: Configuration & CLI (In Progress)
- [ ] YAML parser for services.yaml
- [ ] CLI command structure
- [ ] Service dependency graph resolution
- [ ] Interactive service selection
- [ ] Log streaming and aggregation

### Phase 6: Production Features (Q1 2025)
- [ ] Observability stack integration
- [ ] Advanced deployment strategies
- [ ] Service mesh capabilities
- [ ] Multi-cloud support

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
