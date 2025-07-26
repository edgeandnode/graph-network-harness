# service-orchestration

Heterogeneous service orchestration implementing ADR-007. Manages service lifecycles, health monitoring, and deployment across multiple execution backends.

## Overview

This crate implements the orchestration layer that coordinates services across diverse infrastructure:

- **Lifecycle Management**: Start, stop, restart services with dependency ordering
- **Health Monitoring**: Continuous health checks with configurable intervals
- **Package Deployment**: Deploy pre-built service packages to remote hosts
- **State Management**: Persistent service state and configuration
- **Multi-Backend Support**: Local processes, Docker containers, SSH remotes, systemd services

## Core Concepts

### ServiceConfig

Defines how services should be deployed and managed:

```rust
use service_orchestration::{ServiceConfig, ExecutionBackend, HealthCheck};

let config = ServiceConfig {
    name: "api-server".to_string(),
    backend: ExecutionBackend::Process {
        binary: "./target/release/api-server".to_string(),
        args: vec!["--port".to_string(), "8080".to_string()],
        env: HashMap::from([
            ("DATABASE_URL".to_string(), "postgres://localhost/api".to_string()),
        ]),
    },
    dependencies: vec!["postgres".to_string()],
    health_check: Some(HealthCheck::Http {
        url: "http://localhost:8080/health".to_string(),
        interval_secs: 10,
        timeout_secs: 5,
        retries: 3,
    }),
};
```

### Orchestrator

Manages service lifecycles with dependency resolution:

```rust
use service_orchestration::{Orchestrator, OrchestratorConfig};

let config = OrchestratorConfig {
    state_dir: PathBuf::from("/var/lib/harness"),
    max_parallel_operations: 4,
};

let orchestrator = Orchestrator::new(config).await?;

// Register services
orchestrator.register_service(postgres_config).await?;
orchestrator.register_service(api_config).await?;

// Start with dependency ordering
orchestrator.start_service("api-server").await?;
// Automatically starts postgres first

// Monitor health
let health = orchestrator.get_service_health("api-server").await?;
```

## Execution Backends

### Local Process

Run services as local processes with PID tracking:

```yaml
backend:
  type: process
  binary: ./bin/my-service
  args: ["--config", "/etc/service.conf"]
  env:
    LOG_LEVEL: debug
  working_dir: /opt/service
```

### Docker Container

Manage Docker containers with full lifecycle support:

```yaml
backend:
  type: docker
  image: myapp:latest
  ports:
    - "8080:8080"
  volumes:
    - /data:/var/lib/app
  env:
    NODE_ENV: production
```

### SSH Remote

Deploy to remote hosts via SSH:

```yaml
backend:
  type: ssh
  host: 192.168.1.100
  user: deploy
  key_file: ~/.ssh/deploy_key
  binary: /opt/app/bin/server
  env:
    PORT: 8080
```

### Package Deploy

Deploy pre-built packages to remote hosts:

```yaml
backend:
  type: package
  host: prod-server.example.com
  package: ./releases/app-v1.2.3.tar.gz
  install_path: /opt/app
  pre_install:
    - "sudo systemctl stop app"
  post_install:
    - "sudo systemctl start app"
```

## Health Checks

Multiple health check strategies:

### HTTP Health Check
```yaml
health_check:
  type: http
  url: http://localhost:8080/health
  expected_status: 200
  interval_secs: 30
```

### TCP Port Check
```yaml
health_check:
  type: tcp
  port: 5432
  timeout_secs: 10
```

### Command Check
```yaml
health_check:
  type: command
  command: "pg_isready"
  args: ["-h", "localhost"]
  expected_exit_code: 0
```

### Custom Script
```yaml
health_check:
  type: script
  path: ./scripts/check-health.sh
  interval_secs: 60
```

## Dependency Management

Services can declare dependencies that are automatically resolved:

```rust
let web_config = ServiceConfig {
    name: "web".to_string(),
    dependencies: vec!["api".to_string()],
    // ...
};

let api_config = ServiceConfig {
    name: "api".to_string(),
    dependencies: vec!["db".to_string(), "cache".to_string()],
    // ...
};

// Starting "web" will start db -> cache -> api -> web in order
orchestrator.start_service("web").await?;
```

## State Persistence

Service state is persisted for reliability:

```
/var/lib/harness/
├── services/
│   ├── postgres/
│   │   ├── config.yaml
│   │   ├── state.json
│   │   └── health.log
│   └── api-server/
│       ├── config.yaml
│       ├── state.json
│       └── pid
└── packages/
    └── api-server-v1.2.3.tar.gz
```

## Package Format

Service packages follow a standard format:

```
service-package.tar.gz
├── manifest.yaml       # Package metadata
├── bin/               # Executables
│   └── service
├── config/            # Default configuration
│   └── service.yaml
├── scripts/           # Lifecycle scripts
│   ├── pre-start.sh
│   ├── post-start.sh
│   └── health-check.sh
└── lib/               # Dependencies
    └── libcustom.so
```

## Event System

Subscribe to orchestration events:

```rust
use service_orchestration::{OrchestratorEvent, EventSubscriber};

let mut events = orchestrator.subscribe().await?;

while let Ok(event) = events.recv().await {
    match event {
        OrchestratorEvent::ServiceStarted { name, .. } => {
            println!("Service {} started", name);
        }
        OrchestratorEvent::ServiceFailed { name, error, .. } => {
            eprintln!("Service {} failed: {}", name, error);
        }
        OrchestratorEvent::HealthCheckFailed { name, .. } => {
            eprintln!("Health check failed for {}", name);
        }
        // ... handle other events
    }
}
```

## Configuration File

Complete service orchestration from YAML:

```yaml
version: "1.0"

# Global settings
settings:
  state_dir: /var/lib/harness
  max_parallel: 4
  default_timeout: 300

# Network configuration
networks:
  production:
    type: lan
    subnet: 192.168.1.0/24
    
  monitoring:
    type: wireguard
    config: /etc/wireguard/wg0.conf

# Service definitions
services:
  postgres:
    backend:
      type: docker
      image: postgres:15
      ports: ["5432:5432"]
      env:
        POSTGRES_PASSWORD: secret
    health_check:
      type: command
      command: pg_isready
      
  api:
    backend:
      type: process
      binary: ./bin/api-server
      env:
        DATABASE_URL: postgresql://localhost:5432/api
    dependencies: [postgres]
    health_check:
      type: http
      url: http://localhost:8080/health
      
  monitoring:
    backend:
      type: ssh
      host: monitor.example.com
      binary: /opt/prometheus/prometheus
    network: monitoring
```

## Runtime Agnostic

The orchestrator works with any async runtime:

```rust
// With Tokio
#[tokio::main]
async fn main() {
    let orchestrator = Orchestrator::new(config).await?;
}

// With async-std
#[async_std::main]
async fn main() {
    let orchestrator = Orchestrator::new(config).await?;
}

// With smol
fn main() -> Result<()> {
    smol::block_on(async {
        let orchestrator = Orchestrator::new(config).await?;
    })
}
```

## Testing

The crate includes comprehensive tests using Docker:

```bash
# Run all tests
cargo test -p service-orchestration

# Run with Docker backend tests
cargo test -p service-orchestration --features docker-tests

# Run with SSH tests
cargo test -p service-orchestration --features ssh-tests
```

## License

Licensed under MIT or Apache-2.0, at your option.