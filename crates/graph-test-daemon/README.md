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

Manages Graph Node instances for subgraph indexing:

**Actions:**
- `DeploySubgraph`: Deploy a new subgraph from IPFS
- `QuerySubgraph`: Execute GraphQL queries
- `RemoveSubgraph`: Remove a deployed subgraph

**Events:**
- `DeploymentStarted`: Subgraph deployment initiated
- `DeploymentProgress`: Indexing progress updates
- `DeploymentCompleted`: Subgraph ready for queries
- `QueryResult`: GraphQL query response
- `Error`: Service errors

### Anvil Service

Local Ethereum blockchain for deterministic testing:

**Actions:**
- `MineBlocks`: Mine a specific number of blocks
- `SetBalance`: Set account balances
- `Fork`: Fork from another chain at a specific block

**Events:**
- `BlockMined`: New block created
- `BalanceUpdated`: Account balance changed
- `ForkCreated`: Fork established
- `Error`: Service errors

### PostgreSQL Service

Database service for Graph Node storage:

**Actions:**
- `CreateDatabase`: Create a new database
- `RunQuery`: Execute SQL queries
- `Backup`: Create database backup

**Events:**
- `DatabaseCreated`: Database ready
- `QueryResult`: SQL query results
- `BackupCompleted`: Backup finished
- `Error`: Service errors

### IPFS Service

InterPlanetary File System for subgraph storage:

**Actions:**
- `AddContent`: Add content to IPFS
- `GetContent`: Retrieve content by hash
- `PinContent`: Pin content to prevent garbage collection
- `UnpinContent`: Unpin content

**Events:**
- `ContentAdded`: Content stored with hash
- `ContentRetrieved`: Content fetched
- `ContentPinned`: Content pinned successfully
- `ContentUnpinned`: Content unpinned
- `Error`: Service errors

## Usage

### Starting the Daemon

```bash
# Run the graph-test-daemon
graph-test-daemon --endpoint 127.0.0.1:9443

# The daemon will start with all Graph Protocol services registered
```

### Test Scenarios

#### Deploy and Test a Subgraph

```rust
use graph_test_daemon::{GraphNodeAction, AnvilAction};

// 1. Deploy contracts on Anvil
let deploy_contracts = AnvilAction::SetBalance {
    address: "0x123...".to_string(),
    balance: "1000000000000000000".to_string(), // 1 ETH
};

// 2. Mine some blocks
let mine_blocks = AnvilAction::MineBlocks {
    count: 10,
    interval_secs: Some(1),
};

// 3. Deploy subgraph
let deploy_subgraph = GraphNodeAction::DeploySubgraph {
    name: "uniswap-v3".to_string(),
    ipfs_hash: "QmHash...".to_string(),
    version_label: Some("v1.0.0".to_string()),
};

// 4. Wait for indexing (via events)
// Events: DeploymentStarted -> DeploymentProgress -> DeploymentCompleted

// 5. Query the subgraph
let query = GraphNodeAction::QuerySubgraph {
    subgraph_name: "uniswap-v3".to_string(),
    query: r#"{
        pools(first: 10) {
            id
            token0 { symbol }
            token1 { symbol }
            liquidity
        }
    }"#.to_string(),
};
```

#### Test Blockchain Reorganization

```rust
// 1. Mine initial blocks
let initial_blocks = AnvilAction::MineBlocks {
    count: 100,
    interval_secs: None,
};

// 2. Create a fork at block 50
let fork = AnvilAction::Fork {
    url: "http://localhost:8545".to_string(),
    block_number: Some(50),
};

// 3. Mine alternative chain
let alt_blocks = AnvilAction::MineBlocks {
    count: 60,
    interval_secs: None,
};

// 4. Observe subgraph reorg handling via events
```

## JSON Schemas

All actions and events have JSON schemas available for validation and client generation:

```rust
use schemars::schema_for;

// Get action schema
let action_schema = schema_for!(GraphNodeAction);

// Get event schema  
let event_schema = schema_for!(GraphNodeEvent);
```

## Configuration

Services can be configured through environment variables:

- `GRAPH_NODE_ENDPOINT`: GraphNode HTTP/WebSocket endpoint
- `ANVIL_PORT`: Anvil RPC port (default: 8545)
- `POSTGRES_HOST`: PostgreSQL host
- `POSTGRES_PORT`: PostgreSQL port
- `IPFS_API_PORT`: IPFS API port (default: 5001)

## Integration with Test Frameworks

The daemon integrates seamlessly with Rust test frameworks:

```rust
#[tokio::test]
async fn test_subgraph_indexing() {
    // Connect to daemon
    let client = DaemonClient::connect("127.0.0.1:9443").await?;
    
    // Deploy subgraph
    let events = client.invoke_service_action(
        "graph-node-1",
        serde_json::to_value(GraphNodeAction::DeploySubgraph {
            name: "test-subgraph".to_string(),
            ipfs_hash: "QmTest...".to_string(),
            version_label: None,
        })?
    ).await?;
    
    // Wait for deployment
    while let Ok(event) = events.recv().await {
        match serde_json::from_value::<GraphNodeEvent>(event)? {
            GraphNodeEvent::DeploymentCompleted { .. } => break,
            GraphNodeEvent::Error { message } => panic!("Deployment failed: {}", message),
            _ => continue,
        }
    }
    
    // Run test queries...
}
```

## Development

### Adding New Services

1. Define action and event types
2. Implement the `Service` trait
3. Register in `GraphTestDaemon::new()`

### Testing

```bash
# Run unit tests
cargo test -p graph-test-daemon

# Run with example configuration
RUST_LOG=debug graph-test-daemon
```

## License

Licensed under MIT or Apache-2.0, at your option.