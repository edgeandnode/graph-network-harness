# Graph Network Harness

**A flexible service orchestration framework** - Build systems that manage services across any infrastructure: local processes, Docker containers, remote SSH hosts, or any combination. Create your own specialized harnesses for testing, deployment, monitoring, or operations.

## 🎯 What Is This?

At its core, this is a **heterogeneous service orchestration framework** that provides:
- A strongly-typed `Service` trait for defining actionable services
- Multiple execution backends (local, Docker, SSH, systemd)
- Network topology awareness (Local, LAN, WireGuard)
- WebSocket-based daemon architecture for real-time control

The `graph-test-daemon` included in this repo is just one example of what you can build - a specialized harness for testing Graph Protocol components. But you can create your own domain-specific harnesses for any use case.

## 🚀 What Can You Build?

### Integration Test Harnesses
Like `graph-test-daemon`, create specialized test environments:
- **Blockchain Testing**: Orchestrate multiple chains, bridges, and validators
- **Microservice Testing**: Spin up entire service meshes locally
- **Database Testing**: Coordinate replicated databases with failover scenarios
- **Network Testing**: Simulate complex network topologies and partitions

### Deployment & Operations Tools
Build production-grade orchestration systems:
- **Multi-Cloud Deployers**: Deploy across AWS, GCP, and on-premise
- **Edge Computing**: Manage services across edge locations
- **Hybrid Orchestrators**: Coordinate Kubernetes, VMs, and bare metal
- **Service Migrations**: Orchestrate zero-downtime migrations

### Development Environments
Create sophisticated local development setups:
- **Full-Stack Environments**: Database, backend, frontend, and supporting services
- **Multi-Service Debugging**: Coordinate services for debugging sessions
- **Performance Testing**: Orchestrate load generators and monitoring

### Custom Domain Harnesses
Build specialized harnesses for your domain:
- **IoT Fleet Management**: Orchestrate device simulators and gateways
- **ML Pipeline Orchestration**: Coordinate training, serving, and data services
- **Game Server Management**: Orchestrate game servers, matchmaking, and databases
- **Media Processing**: Coordinate encoding, streaming, and CDN services

## 🚀 Quick Start

```bash
# Install the harness
cargo install --path crates/harness

# Create a service configuration
cat > services.yaml << 'EOF'
version: "1.0"
services:
  postgres:
    type: docker
    image: "postgres:15"
    env:
      POSTGRES_PASSWORD: "secret"
    health_check:
      command: "pg_isready"
      
  my-app:
    type: process
    binary: "./target/release/my-app"
    env:
      DATABASE_URL: "postgresql://postgres:secret@${postgres.ip}:5432/myapp"
    dependencies:
      - postgres
EOF

# Start your services
harness start

# Check status
harness status --detailed

# Run your tests
cargo test --workspace

# Clean up
harness stop
```

## 🔥 Key Features

### Service Orchestration
- **Dependency Management**: Automatically start services in the correct order
- **Health Checks**: Wait for services to be ready before proceeding
- **Cross-Network Discovery**: Services find each other regardless of location
- **Environment Injection**: Automatic configuration with service IPs and ports

### Multiple Execution Backends
- **Local Process**: Direct execution with PID tracking
- **Docker**: Full container lifecycle management
- **SSH Remote**: Deploy to any SSH-accessible host
- **Package Deploy**: Ship pre-built binaries to remote hosts

### Network Topology Support
- **Local**: Services on the same machine
- **LAN**: Services across your local network
- **WireGuard**: Secure tunnels for remote services
- **Mixed**: Any combination of the above

### Developer Experience
- **Simple YAML Config**: Define your entire stack in one file
- **Runtime Agnostic**: Works with Tokio, async-std, or smol
- **Type-Safe APIs**: Strongly typed service actions and events
- **Real-time Monitoring**: WebSocket-based event streaming

## 📚 Example: Testing a Distributed System

```yaml
# services.yaml - A complete microservices test environment
version: "1.0"

services:
  # Database on local Docker
  postgres:
    type: docker
    image: "postgres:15"
    env:
      POSTGRES_PASSWORD: "${DB_PASSWORD}"
    health_check:
      command: "pg_isready"
      interval: 10
      retries: 5

  # API server on remote host
  api:
    type: remote
    host: "192.168.1.100"
    binary: "/opt/api/bin/server"
    env:
      DATABASE_URL: "postgresql://postgres:${DB_PASSWORD}@${postgres.ip}:5432/api"
    dependencies:
      - postgres
    health_check:
      http: "http://localhost:8080/health"

  # Worker process locally
  worker:
    type: process
    binary: "./target/release/worker"
    env:
      API_URL: "http://${api.ip}:8080"
      QUEUE_URL: "redis://${redis.ip}:6379"
    dependencies:
      - api
      - redis

  # Cache on Docker
  redis:
    type: docker
    image: "redis:7"
    health_check:
      tcp: 6379
```

## 🧪 Example: Graph Protocol Testing Harness

The included `graph-test-daemon` demonstrates how to build a specialized harness for Graph Protocol testing:

```rust
// Start a complete Graph Protocol test environment
let daemon = GraphTestDaemon::new("127.0.0.1:9443").await?;
daemon.start().await?;

// Deploy a subgraph
let deploy_action = GraphNodeAction::DeploySubgraph {
    name: "my-subgraph".to_string(),
    ipfs_hash: "QmHash...".to_string(),
    version_label: Some("v1.0.0".to_string()),
};

// Mine some blocks on Anvil
let mine_action = AnvilAction::MineBlocks {
    count: 10,
    interval_secs: Some(1),
};

// Query the subgraph
let query_action = GraphNodeAction::QuerySubgraph {
    subgraph_name: "my-subgraph".to_string(),
    query: "{ tokens { id, name } }".to_string(),
};
```

## 🔨 Building Your Own Harness

Create a specialized harness for your domain by extending `BaseDaemon`:

```rust
use harness_core::{BaseDaemon, Service, ServiceStack};

// Define your domain-specific services
struct DatabaseService { /* ... */ }
struct CacheService { /* ... */ }
struct ApiService { /* ... */ }

// Implement the Service trait for each
impl Service for DatabaseService {
    type Action = DatabaseAction;
    type Event = DatabaseEvent;
    // ... implement dispatch_action
}

// Create your specialized daemon
pub struct MyDomainDaemon {
    base: BaseDaemon,
}

impl MyDomainDaemon {
    pub async fn new(endpoint: SocketAddr) -> Result<Self> {
        let mut builder = BaseDaemon::builder()
            .with_endpoint(endpoint);
            
        // Register your services
        let stack = builder.service_stack_mut();
        stack.register("db-1", DatabaseService::new())?;
        stack.register("cache-1", CacheService::new())?;
        stack.register("api-1", ApiService::new())?;
        
        let base = builder.build().await?;
        Ok(Self { base })
    }
}
```

Your harness can then expose domain-specific functionality while leveraging all the infrastructure provided by harness-core.

## 🏗️ Architecture

The harness uses a modular architecture with clear separation of concerns:

```
┌─────────────────────────────────────────────────┐
│                    CLI                          │
│         harness start, stop, status             │
└─────────────────┬───────────────────────────────┘
                  │ WebSocket (TLS)
┌─────────────────▼───────────────────────────────┐
│                 Daemon                          │
│  ┌─────────────┐ ┌──────────────┐ ┌──────────┐ │
│  │   Service   │ │   Service    │ │  Action  │ │
│  │   Manager   │ │   Registry   │ │  Router  │ │
│  └─────────────┘ └──────────────┘ └──────────┘ │
└─────────────────────────────────────────────────┘
                  │
      ┌───────────┼───────────┬──────────┐
      ▼           ▼           ▼          ▼
┌──────────┐ ┌─────────┐ ┌────────┐ ┌────────┐
│ Process  │ │ Docker  │ │  SSH   │ │Package │
│ Executor │ │Executor │ │Executor│ │Deploy  │
└──────────┘ └─────────┘ └────────┘ └────────┘
```

### Core Components

- **harness-core**: Service trait, action system, and daemon infrastructure
- **command-executor**: Runtime-agnostic command execution across backends
- **service-registry**: Service discovery and network topology management
- **service-orchestration**: Lifecycle management and health monitoring
- **graph-test-daemon**: Graph Protocol specific test services

## 🛠️ Advanced Usage

### Custom Services

Implement the `Service` trait to create your own service types:

```rust
use harness_core::{Service, Result};
use async_trait::async_trait;
use async_channel::Receiver;

struct MyService {
    config: MyConfig,
}

#[async_trait]
impl Service for MyService {
    type Action = MyAction;
    type Event = MyEvent;
    
    fn name(&self) -> &str { "my-service" }
    
    fn description(&self) -> &str { 
        "Custom service for specific testing needs" 
    }
    
    async fn dispatch_action(&self, action: Self::Action) -> Result<Receiver<Self::Event>> {
        // Handle actions and return event stream
    }
}
```

### Network Simulation

Test complex network scenarios:

```yaml
networks:
  datacenter:
    type: lan
    subnet: "10.0.1.0/24"
    
  edge:
    type: wireguard
    subnet: "10.99.0.0/24"
    
services:
  # Service in datacenter
  api:
    network: datacenter
    # ...
    
  # Service at edge location  
  edge-cache:
    network: edge
    env:
      UPSTREAM: "http://${api.ip}:8080"
    # ...
```

### Package Deployment

Deploy pre-built packages to remote hosts:

```yaml
services:
  remote-service:
    type: package
    host: "remote.example.com"
    package: "./releases/service-v1.2.3.tar.gz"
    env:
      CONFIG: "/etc/service/config.yaml"
    install_path: "/opt/service"
```

## 📖 Documentation

- [Architecture Overview](docs/ARCHITECTURE.md) - System design and components
- [Service Configuration](docs/CONFIGURATION.md) - Complete YAML reference
- [Network Topologies](docs/NETWORKS.md) - Setting up complex networks
- [Graph Protocol Testing](docs/GRAPH-TESTING.md) - Using graph-test-daemon
- [Custom Services](docs/CUSTOM-SERVICES.md) - Building your own service types
- [API Reference](docs/API.md) - WebSocket protocol and schemas

## 🤝 Contributing

We welcome contributions! Please see:
- [CONTRIBUTING.md](CONTRIBUTING.md) - How to contribute
- [CODE-POLICY.md](CODE-POLICY.md) - Coding standards
- [CLAUDE.md](CLAUDE.md) - AI assistant guidelines

## 📜 License

Licensed under MIT or Apache-2.0, at your option.

## 🙏 Acknowledgments

Built for the Graph Protocol community to enable comprehensive integration testing across heterogeneous infrastructure.