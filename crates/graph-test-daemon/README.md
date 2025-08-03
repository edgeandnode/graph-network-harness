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

```bash
# Start the daemon
graph-test-daemon --endpoint 127.0.0.1:9443
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

Services configured through environment variables:
- `GRAPH_NODE_ENDPOINT`: GraphNode endpoint
- `ANVIL_PORT`: Anvil RPC port (default: 8545)
- `POSTGRES_HOST`: PostgreSQL host
- `IPFS_API_PORT`: IPFS API port (default: 5001)

## Development

To add new services:
1. Define action and event types
2. Implement the `Service` trait
3. Register in `GraphTestDaemon::new()`

See harness-core documentation for the `Service` trait details.