# ADR-011: Graph Test Daemon

## Status
Implemented

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

We have created a specialized `GraphTestDaemon` as the first implementation of the domain-oriented action system defined in [ADR-010](./010-domain-oriented-action-system.md). This daemon extends the base harness functionality with Graph Protocol-specific service types and test actions.

### Key Implementation Decisions

1. **YAML Configuration Required**: No default constructor - all daemon instances require explicit YAML configuration
2. **service-orchestration Integration**: Uses ServiceTarget from service-orchestration for runtime deployment
3. **Service Type Linking**: YAML `service_type` field links runtime config to action implementations  
4. **Dynamic Service Creation**: ServiceRegistry creates services based on YAML configuration
5. **Strongly-typed Configuration**: Uses service-orchestration config structures instead of HashMap<String, Value>

### Architecture

```rust
// crates/graph-test-daemon/src/daemon.rs
use harness_core::prelude::*;
use service_orchestration::{ServiceConfig, ServiceTarget};

pub struct GraphTestDaemon {
    base: BaseDaemon,
}

impl GraphTestDaemon {
    /// Create from YAML configuration file (required - no default constructor)
    pub async fn from_config<P: AsRef<Path>>(endpoint: SocketAddr, config_path: P) -> Result<Self> {
        let config_content = std::fs::read_to_string(config_path.as_ref())?;
        let config: GraphStackConfig = serde_yaml::from_str(&config_content)?;
        Self::from_stack_config(endpoint, config).await
    }

    /// Create from parsed stack configuration
    pub async fn from_stack_config(endpoint: SocketAddr, config: GraphStackConfig) -> Result<Self> {
        let mut builder = BaseDaemon::builder().with_endpoint(endpoint);
        
        // Register services based on service_type in YAML
        {
            let stack = builder.service_stack_mut();
            for (instance_name, service_config) in &config.services {
                match service_config.service_type.as_str() {
                    "postgres" => {
                        let postgres = PostgresService::new(db_name, port);
                        stack.register(instance_name.clone(), postgres)?;
                    }
                    "anvil" => {
                        let anvil = AnvilService::new(chain_id, port);
                        stack.register(instance_name.clone(), anvil)?;
                    }
                    // ... other service types
                }
            }
        }
        
        // Register high-level actions
        builder = builder
            .register_action("setup-test-stack", "Set up Graph Protocol test stack", action_fn)?
            .register_action("health-check-stack", "Check health of all services", action_fn)?;
            
        let base = builder.build().await?;
        Ok(Self { base })
    }
}
```

### YAML Configuration Structure

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
```

### Service Type Implementation

```rust
// Each service_type maps to a Service implementation
pub struct AnvilService {
    chain_id: u64,
    port: u16,
}

#[async_trait]
impl Service for AnvilService {
    fn name(&self) -> &str { "anvil" }
    fn description(&self) -> &str { "Ethereum test blockchain" }
    
    fn available_actions(&self) -> Vec<ActionInfo> {
        vec![
            ActionInfo { name: "mine-blocks".to_string(), description: "Mine blocks".to_string() },
            ActionInfo { name: "set-balance".to_string(), description: "Set account balance".to_string() },
        ]
    }
    
    async fn handle_action(&self, action: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        match action {
            "mine-blocks" => self.mine_blocks(params).await,
            "set-balance" => self.set_balance(params).await,
            _ => Err(Error::action_not_found(action))
        }
    }
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
2. **YAML Configuration**: Declarative service configuration with strong typing
3. **service-orchestration Integration**: Leverages existing infrastructure for service lifecycle  
4. **Service Type Linking**: Clear mapping between runtime config and action implementations
5. **Better Error Handling**: Clear error messages instead of silent bash failures
6. **Extensibility**: Easy to add new Graph-specific service types and actions
7. **Debugging**: Full visibility into test execution with structured logging
8. **Reproducibility**: Deterministic test scenarios with YAML-defined service stacks

### Negative

1. **Domain Coupling**: Tightly coupled to Graph Protocol concepts
2. **Configuration Overhead**: Requires YAML configuration for all use cases (no defaults)
3. **Maintenance**: Must update when Graph Protocol or service-orchestration changes
4. **Learning Curve**: Developers need to understand harness, service-orchestration, and Graph concepts
5. **Binary Size**: Includes Graph-specific dependencies

### Neutral

1. **Graph Protocol Specific**: Not useful for non-Graph testing
2. **Requires local-network**: Still depends on Graph Protocol docker images
3. **Test-Only Features**: Many features only make sense in test context

## Implementation Status

**COMPLETED**: The graph-test-daemon has been implemented using the service-orchestration architecture.

### Completed Implementation

#### ✅ Core Infrastructure
- Created `graph-test-daemon` crate with YAML configuration
- Implemented `GraphTestDaemon` using `BaseDaemon` from harness-core
- Integrated with service-orchestration for service lifecycle management
- Added service type linking via YAML `service_type` field

#### ✅ Service Types  
- **AnvilService**: Ethereum test blockchain with mining controls
- **PostgresService**: Database management and queries
- **IpfsService**: Content storage and retrieval
- **GraphNodeService**: Subgraph deployment and indexing (basic)

#### ✅ Configuration System
- YAML-based service stack configuration
- ServiceTarget integration for deployment types (Process, Docker, SSH planned)
- Health check configuration support
- Environment variable and argument extraction

#### ✅ Action Framework
- Service-specific actions through Service trait
- High-level daemon actions (`setup-test-stack`, `health-check-stack`)
- Action discovery and invocation through harness-core

### Future Enhancements
- Advanced GraphNode actions (deploy-subgraph, query-subgraph)
- IndexerAgent service for allocation management  
- Advanced blockchain actions (inject-reorg, simulate-load)
- Test scenario builder API
- Performance benchmarking capabilities

## Example Usage

```rust
use harness_core::TestClient;
use graph_test_daemon::GraphTestDaemon;

#[tokio::test]
async fn test_subgraph_reorg_handling() {
    // Start the Graph test daemon with required configuration
    let endpoint = "127.0.0.1:9443".parse().unwrap();
    let daemon = GraphTestDaemon::from_config(endpoint, "test-configs/reorg-test.yaml").await.unwrap();
    daemon.start().await.unwrap();
    
    // Connect test client using harness_core
    let client = TestClient::connect(daemon.endpoint()).await.unwrap();
    
    // Setup Graph Protocol stack (services already configured in YAML)
    client.action("setup-test-stack", json!({})).await.unwrap();
    
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