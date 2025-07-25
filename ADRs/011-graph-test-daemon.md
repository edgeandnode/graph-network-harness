# ADR-011: Graph Test Daemon

## Status
Proposed

## Context

The Graph Protocol integration testing currently requires:
1. Manual orchestration of 20+ interdependent services via docker-compose
2. Complex bash scripts for common operations (deploying subgraphs, creating allocations)
3. Brittle polling loops waiting for services to reach desired states
4. String manipulation and JSON parsing in shell scripts
5. No type safety or proper error handling in test workflows

Analysis of the `local-network` repository reveals patterns that could be automated:
- Multi-step subgraph deployment (build → IPFS upload → hex conversion → contract call)
- Block mining to advance epochs and unstick services
- Allocation management through indexer-agent CLI
- State verification through GraphQL queries
- Payment flow testing with TAP contracts

The current approach has several pain points:
- **Fragility**: Tests break when service startup order changes
- **Debugging difficulty**: Hard to determine why a test failed
- **No reusability**: Each test reimplements common workflows
- **Performance**: Excessive shelling out and polling
- **Maintenance burden**: Bash scripts are hard to refactor

## Decision

We will create a specialized `GraphTestDaemon` as the first implementation of the domain-oriented action system defined in [ADR-010](./010-domain-oriented-action-system.md). This daemon extends the base harness functionality with Graph Protocol-specific service types and test actions.

### Architecture

```rust
// crates/graph-test-daemon/src/lib.rs
use harness_core::{BaseDaemon, Daemon, ServiceType, Action, Result};

pub struct GraphTestDaemon {
    base: BaseDaemon,
    graph_state: Arc<Mutex<GraphTestState>>,
}

impl Daemon for GraphTestDaemon {
    fn new() -> Result<Self> {
        let base = BaseDaemon::builder()
            // Leverage existing harness services
            .with_service_manager()
            .with_service_registry()
            .with_config_parser()
            
            // Graph-specific service types
            .register_service_type::<AnvilBlockchain>()
            .register_service_type::<GraphNode>()
            .register_service_type::<IndexerAgent>()
            .register_service_type::<IPFSNode>()
            .register_service_type::<GatewayService>()
            
            // High-level test actions
            .register_action("deploy-subgraph", Self::deploy_subgraph)
            .register_action("create-allocation", Self::create_allocation)
            .register_action("mine-blocks", Self::mine_blocks)
            .register_action("verify-indexing", Self::verify_indexing_status)
            .register_action("inject-reorg", Self::inject_blockchain_reorg)
            .build()?;
            
        Ok(Self {
            base,
            graph_state: Arc::new(Mutex::new(GraphTestState::new())),
        })
    }
    
    fn base(&self) -> &BaseDaemon {
        &self.base
    }
}
```

### Domain-Specific Service Types

```rust
#[derive(ServiceType)]
#[service_type(name = "anvil-blockchain")]
pub struct AnvilBlockchain {
    pub chain_id: u32,
    pub block_time: Option<u64>,
    pub accounts: Vec<Account>,
    
    #[service_type(test_only)]
    pub auto_mine: bool,
    
    #[service_type(test_only)]
    pub fork_url: Option<String>,
}

#[derive(ServiceType)]
#[service_type(name = "graph-node")]
pub struct GraphNode {
    pub ethereum_rpc: String,
    pub ipfs_endpoint: String,
    pub postgres_url: String,
    
    #[service_type(enriched)]
    pub indexing_status_endpoint: String,
    
    #[service_type(enriched)]
    pub admin_endpoint: String,
    
    #[service_type(test_only)]
    pub log_level: String,
}
```

### High-Level Test Actions

```rust
impl GraphTestDaemon {
    /// Deploy a subgraph with automatic IPFS upload and GNS registration
    async fn deploy_subgraph(&self, params: serde_json::Value) -> Result<serde_json::Value> {
        let params: DeploySubgraphParams = serde_json::from_value(params)?;
        
        // 1. Build subgraph with graph-cli
        let build_output = self.build_subgraph(&params.manifest_path).await?;
        
        // 2. Extract IPFS hash from build output
        let ipfs_hash = self.extract_ipfs_hash(&build_output)?;
        
        // 3. Convert to hex for contract call
        let deployment_hex = self.ipfs_to_hex(&ipfs_hash).await?;
        
        // 4. Call GNS contract to publish
        let tx_hash = self.publish_to_gns(deployment_hex).await?;
        
        // 5. Track deployment in test state
        let mut state = self.graph_state.lock().await;
        state.track_deployment(DeploymentInfo {
            name: params.name,
            ipfs_hash: ipfs_hash.clone(),
            tx_hash: tx_hash.clone(),
            timestamp: Utc::now(),
        });
        
        Ok(serde_json::json!({
            "deployment_id": ipfs_hash,
            "transaction": tx_hash,
        }))
    }
    
    /// Mine blocks with epoch awareness
    async fn mine_blocks(&self, params: serde_json::Value) -> Result<serde_json::Value> {
        let params: MineBlocksParams = serde_json::from_value(params)?;
        
        // Get blockchain service through ServiceManager
        let service_manager = self.base.service_manager();
        let blockchain = service_manager
            .get_service_info("chain")
            .await?
            .ok_or("Blockchain service not found")?;
            
        let mut blocks_mined = 0;
        let mut epochs_advanced = 0;
        
        for _ in 0..params.count {
            // Use action to increase time and mine block
            self.increase_blockchain_time(params.time_increment).await?;
            self.mine_single_block().await?;
            blocks_mined += 1;
            
            // Check if we crossed an epoch boundary
            if blocks_mined % EPOCH_LENGTH == 0 {
                epochs_advanced += 1;
                
                // Trigger epoch-related actions if needed
                if params.trigger_epoch_actions {
                    self.trigger_epoch_actions().await?;
                }
            }
        }
        
        let current_block = self.get_current_block().await?;
        
        Ok(serde_json::json!({
            "blocks_mined": blocks_mined,
            "epochs_advanced": epochs_advanced,
            "current_block": current_block,
        }))
    }
}
```

### Test Scenario API

```rust
// High-level test scenario API using harness_core::TestClient
use harness_core::TestClient;

impl GraphTestDaemon {
    /// Helper for test scenarios
    pub async fn test_grafting_chain(client: &TestClient) -> Result<()> {
        // Deploy base subgraph
        let base = client.action("deploy-subgraph", json!({
            "manifest_path": "subgraphs/base.yaml",
            "name": "base-subgraph"
        })).await?;
        
        // Wait for initial sync
        client.action("wait-for-sync", json!({
            "deployment_id": base["deployment_id"],
            "target_block": 100
        })).await?;
        
        // Create grafting chain
        let mut previous = base["deployment_id"].clone();
        for i in 0..10 {
            let graft = client.action("deploy-grafted-subgraph", json!({
                "base": previous,
                "block": (i + 1) * 250,  // Respect reorg threshold
                "name": format!("graft-{}", i)
            })).await?;
            
            previous = graft["deployment_id"].clone();
        }
        
        // Verify all grafts are properly indexed
        let deployments = client.action("get-graft-chain", json!({})).await?;
        client.action("verify-graft-chain", json!({
            "deployments": deployments["chain"]
        })).await?;
        
        Ok(())
    }
}
```

### State Tracking

```rust
/// Tracks Graph-specific test state
pub struct GraphTestState {
    /// Deployed subgraphs
    deployments: HashMap<String, DeploymentInfo>,
    
    /// Active allocations
    allocations: HashMap<String, AllocationInfo>,
    
    /// Transaction history
    transactions: Vec<TransactionInfo>,
    
    /// Query results for verification
    query_snapshots: HashMap<String, serde_json::Value>,
}

impl GraphTestState {
    /// Get all deployments in a grafting chain
    pub fn get_graft_chain(&self) -> Vec<DeploymentInfo> {
        // Return deployments ordered by graft dependency
    }
    
    /// Snapshot query results for later comparison
    pub fn snapshot_query(&mut self, deployment_id: &str, query: &str, result: serde_json::Value) {
        let key = format!("{}::{}", deployment_id, query);
        self.query_snapshots.insert(key, result);
    }
}
```

## Consequences

### Positive

1. **Type Safety**: All Graph Protocol operations are type-checked at compile time
2. **Workflow Automation**: Complex multi-step processes become single action calls
3. **Better Error Handling**: Clear error messages instead of silent bash failures
4. **Performance**: Direct API calls instead of CLI wrapping
5. **Reproducibility**: Deterministic test scenarios with proper state tracking
6. **Extensibility**: Easy to add new Graph-specific actions
7. **Debugging**: Full visibility into test execution with structured logging

### Negative

1. **Domain Coupling**: Tightly coupled to Graph Protocol concepts
2. **Maintenance**: Must update when Graph Protocol changes
3. **Learning Curve**: Developers need to understand both harness and Graph concepts
4. **Binary Size**: Includes Graph-specific dependencies

### Neutral

1. **Graph Protocol Specific**: Not useful for non-Graph testing
2. **Requires local-network**: Still depends on Graph Protocol docker images
3. **Test-Only Features**: Many features only make sense in test context

## Implementation Plan

**Prerequisites**: This implementation depends on the `harness-core` library defined in ADR-010. The core library must be implemented first to provide the base abstractions (`Daemon`, `ServiceType`, `Action` traits).

### Phase 0: Prerequisites (if not already complete)
- Implement `harness-core` library as defined in ADR-010
- Create `BaseDaemon` with builder pattern
- Implement `ServiceType` derive macro
- Create action discovery protocol

### Phase 1: Core Infrastructure (Week 1)
- Create `graph-test-daemon` crate
- Implement `GraphTestDaemon` with basic service types
- Add `AnvilBlockchain` service with mining controls
- Create `GraphTestState` for tracking deployments

### Phase 2: Service Types (Week 2)
- Implement `GraphNode` with admin API integration
- Add `IndexerAgent` with allocation management
- Create `IPFSNode` with content verification
- Add `GatewayService` with query routing

### Phase 3: Core Actions (Week 3)
- Implement `deploy_subgraph` action
- Add `create_allocation` workflow
- Create `mine_blocks` with epoch awareness
- Add `verify_indexing_status` checks

### Phase 4: Advanced Actions (Week 4)
- Implement `inject_blockchain_reorg`
- Add `simulate_query_load`
- Create `verify_payment_flow`
- Add grafting-specific actions

### Phase 5: Test Scenarios (Week 5)
- Create `TestScenario` builder API
- Implement common test patterns
- Add performance benchmarking
- Create example integration tests

## Example Usage

```rust
use harness_core::TestClient;
use graph_test_daemon::GraphTestDaemon;

#[tokio::test]
async fn test_subgraph_reorg_handling() {
    // Start the Graph test daemon (implements harness_core::Daemon trait)
    let daemon = GraphTestDaemon::new().unwrap();
    daemon.start().await.unwrap();
    
    // Connect test client using harness_core
    let client = TestClient::connect(daemon.base().endpoint()).await.unwrap();
    
    // Setup Graph Protocol stack
    client.action("setup-graph-stack", json!({
        "ethereum_chain_id": 1337,
        "start_block": 0,
        "index_node_ids": true,
    })).await.unwrap();
    
    // Deploy test subgraph
    let deployment = client.action("deploy-subgraph", json!({
        "manifest_path": "test-subgraphs/reorg-test.yaml",
        "name": "reorg-test"
    })).await.unwrap();
    
    // Generate some indexed data
    client.action("generate-test-data", json!({
        "blocks": 20,
        "events_per_block": 10
    })).await.unwrap();
    
    // Wait for indexing
    client.action("wait-for-sync", json!({
        "deployment_id": deployment["deployment_id"],
        "target_block": 20
    })).await.unwrap();
    
    // Snapshot current state
    let pre_reorg = client.action("query-subgraph", json!({
        "deployment_id": deployment["deployment_id"],
        "query": "{ events(first: 1000) { id, block, data } }"
    })).await.unwrap();
    
    // Inject a reorg at block 15
    client.action("inject-reorg", json!({
        "fork_block": 15,
        "new_chain_length": 10,
        "different_events": true
    })).await.unwrap();
    
    // Wait for reorg handling
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // Verify subgraph handled reorg correctly
    let post_reorg = client.action("query-subgraph", json!({
        "deployment_id": deployment["deployment_id"],
        "query": "{ events(first: 1000) { id, block, data } }"
    })).await.unwrap();
    
    // Events from blocks 15-20 should be different
    assert_ne!(pre_reorg["data"]["events"], post_reorg["data"]["events"]);
    
    // Verify integrity
    client.action("verify-reorg-handling", json!({
        "deployment_id": deployment["deployment_id"],
        "fork_block": 15,
        "expected_head": 25
    })).await.unwrap();
}
```

## References

- [ADR-010: Domain-Oriented Action System](./010-domain-oriented-action-system.md) - Defines the library-first architecture this ADR implements
- [ADR-007: Distributed Service Orchestration](./007-distributed-service-orchestration.md) - Base harness architecture
- [The Graph Protocol Documentation](https://thegraph.com/docs/)
- [local-network repository](https://github.com/graphprotocol/local-network) - Current testing approach being replaced
- [graph-node Testing Guide](https://github.com/graphprotocol/graph-node/blob/master/docs/testing.md)