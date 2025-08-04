# graph-test-daemon

A specialized harness daemon for Graph Protocol integration testing. Provides domain-specific services with typed actions and events for automated test workflows.

## Overview

The graph-test-daemon extends harness-core with services tailored for testing Graph Protocol components:

- **GraphNode**: Deploy and query subgraphs
- **Anvil**: Local Ethereum blockchain with deterministic behavior
- **PostgreSQL**: Database for indexer data
- **IPFS**: Decentralized storage for subgraph files

Each service exposes strongly-typed actions and produces events that can be consumed by test frameworks.

## Services

### GraphNode Service
- **Actions**: DeploySubgraph, QuerySubgraph, RemoveSubgraph
- **Events**: DeploymentStarted, DeploymentProgress, DeploymentCompleted, QueryResult

### Anvil Service
- **Actions**: MineBlocks, SetBalance, Fork
- **Events**: BlockMined, BalanceUpdated, ForkCreated

### PostgreSQL Service
- **Actions**: CreateDatabase, RunQuery, Backup
- **Events**: DatabaseCreated, QueryResult, BackupCompleted

### IPFS Service
- **Actions**: AddContent, GetContent, PinContent, UnpinContent
- **Events**: ContentAdded, ContentRetrieved, ContentPinned, ContentUnpinned

## Usage

The daemon requires a YAML configuration file that defines the Graph Protocol service stack:

```bash
# Start the daemon with required configuration
graph-test-daemon --config path/to/config.yaml --endpoint 127.0.0.1:9443
```

### Configuration Format

The YAML configuration defines services with their runtime targets and links them to action implementations via `service_type`:

```yaml
name: graph-test-stack
description: Complete Graph Protocol test environment

services:
  postgres:
    service_type: postgres  # Links to PostgresService for actions
    name: postgres
    target:
      type: Docker
      image: postgres:14
      env:
        POSTGRES_USER: graph-node
        POSTGRES_PASSWORD: graph-password
        POSTGRES_DB: graph-node
      ports: [5432]
    health_check:
      command: pg_isready
      args: ["-U", "graph-node"]
      interval: 5
      timeout: 3
      retries: 5

  anvil:
    service_type: anvil  # Links to AnvilService for actions
    name: anvil
    target:
      type: Process
      binary: anvil
      args: ["--port", "8545", "--chain-id", "31337"]
      env:
        CHAIN_ID: "31337"
    health_check:
      command: curl
      args: ["-s", "http://localhost:8545"]
      interval: 5
```

### Test Example

```rust
use graph_test_daemon::{GraphNodeAction, DaemonClient};

// Connect to daemon
let client = DaemonClient::connect("127.0.0.1:9443").await?;

// Deploy a subgraph
let events = client.invoke_service_action(
    "graph-node-1",
    serde_json::to_value(GraphNodeAction::DeploySubgraph {
        name: "test-subgraph".to_string(),
        ipfs_hash: "QmTest...".to_string(),
        version_label: None,
    })?
).await?;

// Wait for deployment completion
while let Ok(event) = events.recv().await {
    match serde_json::from_value::<GraphNodeEvent>(event)? {
        GraphNodeEvent::DeploymentCompleted { .. } => break,
        GraphNodeEvent::Error { message } => panic!("Failed: {}", message),
        _ => continue,
    }
}
```

## Configuration

### Service Type Linking

Each service in the YAML configuration uses a `service_type` field that links the runtime configuration to action implementations:

- **`postgres`**: PostgresService - Database operations and queries
- **`anvil`**: AnvilService - Blockchain mining and balance management  
- **`graph-node`**: GraphNodeService - Subgraph deployment and indexing
- **`ipfs`**: IpfsService - Content storage and retrieval

### Service Target Types

Services can be deployed using different target types from service-orchestration:

- **`Process`**: Local process execution with command-executor
- **`Docker`**: Container deployment with Docker CLI
- **`SSH`**: Remote execution (planned)

### Parameter Extraction

Service parameters are extracted from the target configuration:
- **Environment variables**: `target.env` values
- **Process arguments**: `target.args` for process targets
- **Container ports**: `target.ports` for Docker targets

## Development

To add new services:
1. Define action and event types in `services.rs`
2. Implement the `Service` trait with actionable methods
3. Add service creation logic in `GraphTestDaemon::from_stack_config()`
4. Add the new `service_type` to the match statement

### Architecture

The daemon uses a layered architecture:

1. **YAML Configuration**: Defines service instances and their targets
2. **Service Registry**: Maps `service_type` to action implementations  
3. **service-orchestration**: Handles service lifecycle and execution
4. **command-executor**: Provides runtime-agnostic process management

See harness-core documentation for the `Service` trait details and service-orchestration for target types.