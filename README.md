# Graph Network Harness

A heterogeneous service orchestration framework implementing distributed service management across local processes, Docker containers, and remote machines. Built on a runtime-agnostic architecture supporting any async runtime (Tokio, async-std, smol).

## 🚀 Current Status

**Phase 5 of 7 Complete** - Daemon architecture implemented with secure TLS WebSocket communication.

### ✅ What's Working Today

- **Daemon Architecture**: `harness-executor-daemon` with secure TLS WebSocket communication
- **Command Execution**: Run commands locally, via SSH, or in Docker containers
- **Service Registry**: Automatic service discovery with WebSocket-based updates
- **Network Detection**: Identify Local, LAN, and WireGuard network topologies
- **Service Orchestration**: Start/stop services with dependency management
- **Health Monitoring**: Configurable health checks with retry logic
- **Package Deployment**: Deploy service packages to remote hosts
- **IP Management**: Automatic IP allocation within configured subnets
- **TLS Security**: Self-signed certificate management with 1-year expiry

### 🚧 What's Next: Phase 6 - Configuration System

- **YAML Configuration**: Define services in simple YAML files
- **Environment Variables**: Support for ${VAR} substitution
- **Service References**: Use ${service.ip} to reference other services
- **Enhanced CLI**: Full service management commands

### 📋 Coming Soon: Phase 7 - Full CLI

- **Complete CLI**: All commands (logs, exec, deploy, etc.)
- **Great UX**: Progress bars, interactive prompts, helpful errors
- **Shell Completions**: Bash, Zsh, Fish support
- **Production Ready**: <100ms startup, comprehensive docs

### 🔮 Future: Phase 8 - Production Features

- **Auto-scaling**: Scale services based on load
- **Monitoring**: Prometheus/Grafana integration
- **Advanced Deployment**: Rolling updates, blue-green
- **Enterprise**: Multi-region, service mesh, web UI

## Architecture Overview

The harness implements [ADR-007](ADRs/007-distributed-service-orchestration.md) for heterogeneous service orchestration:

```
┌─────────────────────────────────────────────────────────┐
│                        CLI                              │
│            harness daemon status                        │
│            harness service list                         │
└─────────────────┬───────────────────────────────────────┘
                  │ TLS WebSocket (port 9443)
                  ▼
┌─────────────────────────────────────────────────────────┐
│              harness-executor-daemon                    │
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
- **Docker Containers**: Container lifecycle management with resource limits
- **Remote SSH**: Deploy and manage services on LAN nodes
- **WireGuard Networks**: Package-based deployment over secure tunnels

### 🌐 Network Testing Capabilities
- **Multi-Network Topologies**: Test services across local, LAN, and WireGuard boundaries
- **Automatic IP Resolution**: Services find each other regardless of network location
- **Network Isolation**: Test network partitions and failures
- **Cross-Network Dependencies**: Validate service communication across networks

### 🏥 Health Monitoring
- **Multiple Check Types**: HTTP endpoints, TCP ports, or custom commands
- **Configurable Strategies**: Set intervals, timeouts, and retry counts
- **Dependency Awareness**: Health cascades through service dependencies

### 📦 Package Deployment
- **Remote Deployment**: Deploy pre-built packages to any SSH-accessible host
- **Environment Injection**: Automatically inject dependency IPs and configs
- **Zero-Downtime Updates**: Deploy new versions without service interruption

### ⚡ Runtime Agnostic
- **Any Async Runtime**: Works with Tokio, async-std, smol, or custom runtimes
- **No Runtime Lock-in**: Libraries use async-process, async-fs, async-net
- **Test Framework Agnostic**: Use with any test runner or framework

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

### Example: Testing Cross-Network Communication

```yaml
# test-services.yaml
version: "1.0"

networks:
  local:
    type: local
  
  test-lan:
    type: lan
    subnet: "192.168.100.0/24"
    nodes:
      - host: "192.168.100.10"
        name: "test-node-1"
        ssh_user: "test"
        ssh_key: "~/.ssh/test_key"

services:
  test-db:
    type: docker
    network: local
    image: "postgres:15"
    env:
      POSTGRES_PASSWORD: "test"
    health_check:
      command: "pg_isready"
      interval: 5
      retries: 3
      timeout: 5

  test-app:
    type: remote
    network: test-lan
    host: "192.168.100.10"
    binary: "/opt/app/bin/server"
    env:
      DATABASE_URL: "postgresql://postgres:test@${test-db.ip}:5432/app"
    dependencies:
      - test-db
    health_check:
      http: "http://localhost:8080/health"
      interval: 10
      retries: 3
```

Run tests with:
```bash
# Start test environment
harness start -c test-services.yaml

# Run your integration tests
pytest tests/integration/

# Clean up
harness stop --all
```

## Getting Started

### Current Usage (Phase 1-4)

Today, the orchestrator is available as a Rust library:

```toml
[dependencies]
service-orchestration = { path = "crates/service-orchestration" }
```

See the test files in `crates/service-orchestration/tests/` for usage examples.

### Available Now

```bash
# Start the daemon
harness-executor-daemon

# Check daemon status
harness daemon status
```

### Coming Soon (Phase 6)

```bash
# Install harness
cargo install graph-network-harness

# Create services.yaml
cat > services.yaml << 'EOF'
version: "1.0"
services:
  postgres:
    type: docker
    network: local
    image: "postgres:15"
    env:
      POSTGRES_PASSWORD: "secret"
EOF

# Start services
harness start

# Check status  
harness status
```

### Example Configuration

```yaml
# services.yaml
version: "1.0"
name: "my-application"

services:
  database:
    type: docker
    network: local
    image: "postgres:15"
    env:
      POSTGRES_PASSWORD: "${DB_PASSWORD}"
      POSTGRES_DB: "myapp"
    ports:
      - 5432
    health_check:
      command: "pg_isready"
      interval: 10
      retries: 5

  my-api:
    type: process
    network: local
    binary: "./target/release/api-server"
    args: ["--port", "8080"]  
    env:
      DATABASE_URL: "postgresql://postgres:${DB_PASSWORD}@${database.ip}:5432/myapp"
      LOG_LEVEL: "${LOG_LEVEL:-info}"
    dependencies:
      - database
    health_check:
      http: "http://localhost:8080/health"
      interval: 30
      retries: 3
      timeout: 10
```

### Service Management Commands

```bash
# Start all services with dependencies
harness start

# Start specific services
harness start api worker

# Check service status
harness status
┌─────────────┬─────────┬────────────┬─────────┬───────────────┬──────────────┐
│ Service     │ Status  │ Health     │ Network │ Host          │ Uptime       │
├─────────────┼─────────┼────────────┼─────────┼───────────────┼──────────────┤
│ database    │ running │ ✓ healthy  │ local   │ localhost     │ 2h 15m       │
│ my-api      │ running │ ✓ healthy  │ local   │ localhost     │ 2h 15m       │
└─────────────┴─────────┴────────────┴─────────┴───────────────┴──────────────┘

# View logs
harness logs my-api --follow

# Stop services
harness stop --all
```

### Advanced Example: Multi-Network Deployment

```yaml
# services.yaml
version: "1.0"

networks:
  local:
    type: local
  
  production:
    type: lan
    subnet: "192.168.1.0/24"
    nodes:
      - host: "192.168.1.100"
        name: "prod-api"
        ssh_user: "deploy"
        ssh_key: "~/.ssh/prod_key"
      - host: "192.168.1.101"
        name: "prod-worker"
        ssh_user: "deploy"
        ssh_key: "~/.ssh/prod_key"
  
  monitoring:
    type: wireguard
    subnet: "10.0.0.0/24"
    config_path: "/etc/wireguard/monitoring.conf"
    nodes:
      - host: "10.0.0.10"
        name: "metrics-collector"

services:
  # Local infrastructure
  postgres:
    type: docker
    network: local
    image: "postgres:15"
    env:
      POSTGRES_PASSWORD: "${POSTGRES_PASSWORD}"
    volumes:
      - "${DATA_DIR}/postgres:/var/lib/postgresql/data"
    health_check:
      command: "pg_isready"
      interval: 10
      retries: 5

  # Production API on LAN
  api:
    type: remote
    network: production
    host: "192.168.1.100"
    binary: "/opt/api/bin/server"
    args: ["--port", "8080"]
    env:
      DATABASE_URL: "postgresql://postgres:${POSTGRES_PASSWORD}@${postgres.ip}:5432/api"
    dependencies:
      - postgres
    startup_timeout: 300  # 5 minutes for migrations
    health_check:
      http: "http://localhost:8080/health"
      start_period: 60

  # Worker on different LAN node  
  worker:
    type: remote
    network: production
    host: "192.168.1.101"
    binary: "/opt/worker/bin/worker"
    env:
      API_URL: "http://${api.ip}:8080"
      WORKER_THREADS: "${WORKER_THREADS:-4}"
    dependencies:
      - api
    health_check:
      command: "/opt/worker/bin/health-check"
      interval: 60

  # Metrics collector over WireGuard
  metrics:
    type: package
    network: monitoring
    host: "10.0.0.10"
    package: "./packages/metrics-${VERSION}.tar.gz"
    env:
      SCRAPE_TARGETS: "${api.ip}:9090,${worker.ip}:9090"
      RETENTION_DAYS: "30"
    dependencies:
      - api
      - worker
```

## Project Structure

```
graph-network-harness/
├── crates/
│   ├── command-executor/        # Runtime-agnostic command execution
│   ├── service-registry/        # Service discovery & network topology
│   ├── service-orchestration/   # Service lifecycle orchestration
│   ├── harness-config/          # YAML configuration parsing
│   └── harness/                 # CLI and daemon binaries
├── ADRs/                        # Architecture Decision Records
│   ├── 007-distributed-service-orchestration.md
│   └── 007-implementation/
│       └── plan.md             # Implementation phases
├── DAEMON.md                    # Daemon documentation
├── docker-test-env/            # Docker-in-Docker test environment
└── test-activity/              # Test logs and artifacts
```

## Development

### Prerequisites

- Rust toolchain (1.70+)
- Docker (for container execution and testing)
- SSH access (for remote execution testing)

### Building

```bash
# Build all crates
cargo build --workspace

# Run all tests (requires Docker)
cargo test --workspace

# Run specific test suites
cargo test -p command-executor           # Command execution tests
cargo test -p service-registry --features docker-tests  # Network discovery
cargo test -p service-orchestration     # Service lifecycle tests
cargo test -p harness                   # CLI and daemon tests

# Run integration tests only
cargo test --test '*' --workspace
```

### Testing Infrastructure

The harness uses Docker-in-Docker (DinD) for complete test isolation:

```bash
# Test output is saved in timestamped sessions
test-activity/
└── logs/
    ├── current/           # Symlink to latest session
    └── session-20240315-123456/
        ├── harness.log    # Main test log
        ├── services/      # Individual service logs
        └── docker/        # Docker daemon logs
```

### Network Testing with Docker Compose

We simulate complex network topologies using Docker Compose:

```yaml
# docker-compose.test.yml
version: '3.8'

networks:
  lan_network:
    driver: bridge
    ipam:
      config:
        - subnet: 192.168.100.0/24
  
  wan_network:
    driver: bridge
    ipam:
      config:
        - subnet: 10.0.0.0/24

services:
  # Simulated LAN node
  lan_node:
    image: harness-test-node
    networks:
      lan_network:
        ipv4_address: 192.168.100.10
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
  
  # Simulated WireGuard node
  wg_node:
    image: harness-test-node
    networks:
      wan_network:
        ipv4_address: 10.0.0.10
    environment:
      WIREGUARD_CONFIG: /etc/wireguard/wg0.conf
```

### Testing Philosophy

**TEST EARLY AND OFTEN**: Each phase is validated by comprehensive tests before moving forward.

- **Unit Tests**: Test individual components in isolation
- **Integration Tests**: Test with real infrastructure (Docker, SSH)
- **Network Tests**: Simulate multi-network topologies
- **End-to-End Tests**: Full system tests with real services
- **30+ tests** in orchestrator alone, **100+ tests** total

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

### ✅ Recently Implemented (Phase 5)

#### Daemon Architecture
- `harness-executor-daemon` binary with TLS WebSocket server
- Secure certificate management with 1-year expiry
- CLI connects to daemon on port 9443
- Persistent state in `~/.local/share/harness/`
- Structured logging to file

#### Basic CLI Interface
- `harness daemon status` - Check daemon connectivity
- Foundation for service management commands

### ❌ Not Yet Implemented

#### Configuration System (Phase 6)
- YAML service definition parsing
- Environment variable substitution
- Secret management integration
- Configuration validation

#### Enhanced CLI Interface (Phase 6)
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

### Phase 6: Configuration & CLI (In Progress)
- [ ] YAML parser for services.yaml
- [ ] Enhanced CLI command structure
- [ ] Service dependency graph resolution
- [ ] Interactive service selection
- [ ] Log streaming and aggregation

### Phase 7: Production Features
- [ ] Observability stack integration
- [ ] Advanced deployment strategies
- [ ] Service mesh capabilities
- [ ] Multi-cloud support

## Design Principles

1. **Runtime Agnostic**: No hard dependency on any specific async runtime
   - Use `async-process`, `async-fs`, `async-net` instead of runtime-specific versions
   - Test with multiple runtimes to ensure compatibility

2. **Composable**: Each crate can be used independently
   - `command-executor`: Just run commands anywhere
   - `service-registry`: Just track services and networks
   - `orchestrator`: Combine both for full orchestration

3. **Test-Driven**: Every feature has comprehensive test coverage
   - Docker-in-Docker for isolated testing
   - Network simulation with Docker Compose
   - No mocking - test with real infrastructure

4. **Extensible**: Easy to add new capabilities
   - New executor types (Kubernetes, systemd, etc.)
   - Custom health check strategies
   - Plugin system for service types

5. **Production Ready**: Built for real-world use
   - Structured logging with session tracking
   - Graceful error handling and recovery
   - Resource cleanup on failure

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
