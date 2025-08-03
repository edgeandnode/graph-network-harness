# Developer Guide

This guide helps developers understand and contribute to the Graph Network Harness.

## Project Structure

```
graph-network-harness/
├── crates/
│   ├── command-executor/      # Low-level command execution
│   ├── service-registry/      # Service discovery
│   ├── service-orchestration/ # Service lifecycle management
│   ├── harness-config/       # YAML configuration
│   ├── harness-core/         # Core abstractions
│   ├── harness/              # CLI and daemon
│   └── graph-test-daemon/    # Graph Protocol testing
├── docs/                     # Documentation
├── ADRs/                     # Architecture Decision Records
├── xtask/                    # Build automation
└── test-fixtures/           # Test data
```

## Development Setup

```bash
# Clone the repository
git clone https://github.com/graphprotocol/graph-network-harness
cd graph-network-harness

# Run all checks (fmt, clippy, tests)
cargo xtask ci all

# Run specific package tests
cargo xtask test --package command-executor

# Run with Docker tests enabled
cargo test -p service-orchestration --features docker-tests
```

## Key Concepts

### 1. Launchers vs Attachers

```rust
// Launcher: Creates new processes
let launcher = LocalLauncher;
let (events, handle) = launcher.launch(&target, command).await?;
handle.terminate().await?;  // Full lifecycle control

// Attacher: Connects to existing services
let attacher = LocalAttacher;
let (events, handle) = attacher.attach(&target).await?;
handle.status().await?;     // Read-only access
```

### 2. Layered Execution

```rust
// Stack execution layers for complex scenarios
let executor = LayeredExecutor::new(LocalLauncher)
    .with_layer(SshLayer::new("user@host"))      // Outer: SSH
    .with_layer(DockerLayer::new("container"));   // Inner: Docker

// Results in: ssh user@host "docker exec container <command>"
```

### 3. Service Types

```rust
// Define a new service with typed actions
pub struct MyService;

#[derive(Serialize, Deserialize, JsonSchema)]
pub enum MyAction {
    Start { port: u16 },
    Stop,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub enum MyEvent {
    Started { pid: u32 },
    Stopped,
}

impl Service for MyService {
    type Action = MyAction;
    type Event = MyEvent;
    
    async fn dispatch_action(&self, action: Self::Action) -> Result<Receiver<Self::Event>> {
        // Handle action and return event stream
    }
}
```

### 4. Service Configuration

```yaml
# services.yaml
version: "1.0"
services:
  my-service:
    type: process
    binary: ./my-app
    args: ["--port", "8080"]
    env:
      LOG_LEVEL: debug
    health_check:
      command: curl -f http://localhost:8080/health
      interval: 5s
      timeout: 2s
      retries: 3
```

## Common Tasks

### Adding a New Executor

1. Create executor in `service-orchestration/src/executors/`:
```rust
pub struct MyExecutor;

#[async_trait]
impl ServiceExecutor for MyExecutor {
    async fn start(&self, config: ServiceConfig) -> Result<RunningService> {
        // Implementation
    }
    
    async fn stop(&self, service: &RunningService) -> Result<()> {
        // Implementation
    }
    
    async fn stream_events(&self, service: &RunningService) -> Result<EventStream> {
        // Implementation
    }
}
```

2. Register in ServiceManager:
```rust
executors.insert("my-type".to_string(), Arc::new(MyExecutor::new()));
```

### Adding a New Layer

1. Implement ExecutionLayer trait:
```rust
pub struct MyLayer;

impl ExecutionLayer for MyLayer {
    fn wrap_command(&self, command: Command, context: &ExecutionContext) -> Result<Command> {
        // Transform the command
    }
    
    fn description(&self) -> String {
        "My custom layer".to_string()
    }
}
```

### Creating a Domain-Specific Daemon

1. Define your daemon using harness-core:
```rust
use harness_core::prelude::*;

pub struct MyDaemon {
    base: BaseDaemon,
}

impl MyDaemon {
    pub async fn new(endpoint: SocketAddr) -> Result<Self> {
        let base = BaseDaemon::builder()
            .with_endpoint(endpoint)
            .register_service("my-service", MyService::new())
            .register_action("my-action", "Description", |params| async {
                // Action implementation
                Ok(json!({"status": "success"}))
            })
            .build().await?;
            
        Ok(Self { base })
    }
}

#[async_trait]
impl Daemon for MyDaemon {
    async fn start(&self) -> Result<()> {
        self.base.start().await
    }
    // ... other trait methods
}
```

## Testing

### Unit Tests
```rust
#[test]
fn test_my_component() {
    // Synchronous test
}

#[smol_potat::test]
async fn test_async_component() {
    // Async test using smol runtime
}
```

### Integration Tests
```rust
#[smol_potat::test]
#[cfg(feature = "docker-tests")]
async fn test_with_docker() {
    // Test requiring Docker
}
```

### Test Utilities
- `TestHarness`: Manages Docker containers for tests
- `SharedContainer`: Reusable test containers
- `test-activity/logs/`: Session-based test logs

## Debugging

### Enable Tracing
```bash
RUST_LOG=debug cargo test
```

### Check Test Logs
```bash
# Current test session
tail -f test-activity/logs/current/*.log

# Previous sessions
ls test-activity/logs/
```

### Common Issues

1. **Import Errors**: Check if modules were reorganized
   - `backends::local::LocalLauncher` → `backends::LocalLauncher`

2. **Runtime Panics**: Ensure using runtime-agnostic libraries
   - Use `async-process` not `tokio::process`
   - Use `async-net` not `tokio::net`

3. **Docker Tests Failing**: Ensure Docker daemon is running
   ```bash
   docker version
   ```

## Code Style

### Follow CODE-POLICY.md
- Runtime-agnostic design
- No re-exports in lib.rs
- Use `error_set!` for error composition
- Document all public APIs

### Conventions
```rust
// Use thiserror for errors
#[derive(Error, Debug)]
pub enum MyError {
    #[error("Failed to start: {0}")]
    StartFailed(String),
}

// Use async-trait for public APIs
#[async_trait]
pub trait MyTrait {
    async fn my_method(&self) -> Result<()>;
}

// Use #![warn(missing_docs)] in library crates
```

## CI/CD

### Local CI
```bash
# Format check
cargo xtask ci fmt-check

# Clippy
cargo xtask ci clippy

# Tests
cargo xtask ci test

# Everything
cargo xtask ci all
```

### GitHub Actions
- Runs on every PR
- Tests with multiple Rust versions
- Docker-in-Docker for integration tests

## Contributing

1. **Check ADRs**: Understand architectural decisions
2. **Run Tests**: Ensure all tests pass
3. **Update Docs**: Keep documentation current
4. **Follow Style**: Use cargo fmt and clippy
5. **Add Tests**: New features need tests

## Resources

- [Architecture Overview](./ARCHITECTURE.md)
- [Current Status](./CURRENT-STATUS.md)
- [Code Policy](./CODE-POLICY.md)
- [ADRs](../ADRs/README.md)
- [Issues Backlog](./ISSUES-BACKLOG.md)