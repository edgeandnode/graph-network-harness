# Graph Network Harness

A heterogeneous service orchestration framework for testing Graph Protocol components. 

This project started as a testing framework for the Graph Protocol ecosystem but has evolved into a flexible foundation for orchestrating services across different execution environments. Whether you're running local processes, Docker containers, or remote services over SSH, this framework provides the abstractions and tooling to manage them coherently.

## What Is This?

At its core, this is a service orchestration framework that solves a specific problem: how do you test complex distributed systems that span multiple execution environments? In the Graph Protocol world, this means coordinating Graph Nodes, IPFS instances, Ethereum nodes like Anvil, and PostgreSQL databases. But the underlying framework is general enough to handle many other use cases.

The framework provides:
- A strongly-typed Service trait for defining actionable services
- Multiple execution backends (local processes, Docker, SSH, systemd)
- Network topology awareness for services running across different environments
- A WebSocket-based daemon architecture for real-time control and monitoring

## Architecture Overview

The framework is built as a collection of Rust crates, each with a specific responsibility:

- **[harness-core](crates/harness-core/README.md)**: The foundational Service trait and daemon infrastructure
- **[command-executor](crates/command-executor/README.md)**: Runtime-agnostic command execution across different backends
- **[service-registry](crates/service-registry/README.md)**: Service discovery with persistent storage using sled
- **[service-orchestration](crates/service-orchestration/README.md)**: Lifecycle management, health checks, and dependency resolution
- **[graph-test-daemon](crates/graph-test-daemon/README.md)**: The Graph Protocol specific implementation

The architecture emphasizes runtime agnosticism - the core libraries would work with any async runtime (tokio, async-std, smol), though specific implementations other than smol have not yet been completed and you may choose a particular runtime for convenience.

## The Graph Protocol Testing Use Case

The included `graph-test-daemon` demonstrates the framework's capabilities by providing a complete testing environment for Graph Protocol development:

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

// Mine blocks on Anvil
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

This allows developers to write integration tests that exercise the entire Graph Protocol stack locally, with deterministic control over blockchain state and timing.

## Beyond Graph Protocol

While the Graph Protocol testing harness is the reference implementation, the framework is designed to be extended for other domains. The core abstractions are intentionally generic.

Consider a microservices testing scenario where you need to coordinate databases, message queues, and multiple API services. Or imagine a development environment that needs to orchestrate a mix of legacy services running on remote hosts with newer containerized services. The framework provides the building blocks for these scenarios.

The key is the Service trait, which defines a common interface for interacting with any kind of service:

```rust
pub trait Service: Send + Sync + 'static {
    type Action: DeserializeOwned + Send;
    type Event: Serialize + Send;
    
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    async fn dispatch_action(&self, action: Self::Action) -> Result<Receiver<Self::Event>>;
}
```

This simple abstraction, combined with the various execution backends, allows you to build specialized harnesses for your specific needs.

## Getting Started

```bash
# Install the harness CLI
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

## Development

### CI and Testing

See our [CI and Testing Guide](CI.md) for detailed information on running tests and CI checks locally.

Quick start:
```bash
# Run all CI checks
cargo xtask ci all

# Run specific test suite
cargo xtask test --package service-registry
```

## Architecture Overview

The framework is built as a collection of Rust crates, each with a specific responsibility:

- **harness-core**: The foundational Service trait and daemon infrastructure
- **command-executor**: Abstractions for executing commands across different backends
- **service-registry**: Service discovery with persistent storage using sled
- **service-orchestration**: Lifecycle management, health checks, and dependency resolution
- **graph-test-daemon**: The Graph Protocol specific implementation



## Current State and Future Direction

This project is under active development. The core framework is stable but will likely evolve as we build more specialized harnesses and discover new patterns.

## License

Licensed under MIT or Apache-2.0, at your option.
