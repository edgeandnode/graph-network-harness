# Graph Protocol Components: Tasks vs Services

## Overview

Understanding the distinction between tasks and services is critical for proper orchestration:

- **Tasks**: One-time operations that complete and exit. They perform setup, deployment, or configuration actions.
- **Services**: Long-running processes that provide ongoing functionality. They have health checks and can be stopped/started.

## Tasks (One-Time Operations)

### 1. Contract Deployment Tasks

#### graph-contracts
- **Type**: Task
- **Purpose**: Deploy Graph Protocol smart contracts
- **Operations**:
  - Deploy ~12 core contracts (Controller, GNS, Staking, etc.)
  - Save contract addresses to contracts.json
  - Deploy graph-network subgraph
- **Idempotency**: Checks if graph-network subgraph exists
- **Exit**: Completes after deployment

#### tap-contracts
- **Type**: Task  
- **Purpose**: Deploy TAP (Timeline Aggregation Protocol) contracts
- **Operations**:
  - Deploy 3 TAP contracts using forge
  - Save addresses
  - Deploy TAP subgraph
- **Idempotency**: Checks if TAP subgraph exists
- **Exit**: Completes after deployment

### 2. Configuration Tasks

#### indexer-staking
- **Type**: Task
- **Purpose**: Stake the indexer on the protocol
- **Operations**:
  - Check if indexer is already staked
  - Transfer ETH and GRT to indexer
  - Approve GRT spending
  - Call stake() on Staking contract
- **Idempotency**: Checks hasStake() before staking
- **Exit**: Completes after staking

#### subgraph-deploy
- **Type**: Task
- **Purpose**: Deploy and configure core subgraphs
- **Operations**:
  - Get deployment IDs for network, block-oracle, TAP subgraphs
  - Force indexing via reassign
  - Publish to GNS
  - Wait for active allocation
- **Idempotency**: Checks if subgraphs exist
- **Exit**: Completes after allocation is active

### 3. Hybrid Components (Task + Service)

These components perform initial setup tasks, then run as services:

#### block-oracle
- **Task Phase**:
  - Deploy EventfulDataEdge contract
  - Register networks via contract
  - Deploy block-oracle subgraph
  - Generate config.toml
- **Service Phase**: 
  - Run oracle service continuously
  - Monitor blocks and submit attestations

## Services (Long-Running Processes)

### 1. Infrastructure Services

#### anvil
- **Type**: Service
- **Purpose**: Local Ethereum blockchain
- **Health Check**: RPC endpoint responds to eth_blockNumber
- **No Setup Required**: Starts immediately

#### postgres
- **Type**: Service
- **Purpose**: Database for Graph Node
- **Health Check**: pg_isready command
- **Setup**: Database and user creation (handled by container)

#### ipfs
- **Type**: Service
- **Purpose**: Distributed storage for subgraph files
- **Health Check**: API endpoint /api/v0/id
- **Setup**: Configure CORS headers for graph-node

#### redpanda
- **Type**: Service
- **Purpose**: Kafka-compatible message broker
- **Health Check**: Kafka admin API
- **No Setup Required**: Starts immediately

### 2. Core Services

#### graph-node
- **Type**: Service
- **Purpose**: Index and query subgraphs
- **Health Check**: GraphQL endpoint responds
- **Dependencies**: postgres, ipfs, anvil must be healthy
- **Setup**: Connection configuration (automatic)

### 3. Indexer Services

#### indexer-agent
- **Type**: Service (with initial config generation)
- **Purpose**: Manage indexer allocations
- **Initial Task**: Generate config.yaml and tap-contracts.json
- **Health Check**: Management API endpoint
- **Dependencies**: graph-node, block-oracle healthy

#### indexer-service
- **Type**: Service
- **Purpose**: Serve queries and collect payments
- **Health Check**: Service endpoint
- **Dependencies**: indexer-agent, ipfs, tap-escrow-manager

#### tap-agent
- **Type**: Service
- **Purpose**: Submit TAP receipts
- **Health Check**: Agent endpoint
- **Dependencies**: indexer-agent healthy

#### tap-aggregator
- **Type**: Service
- **Purpose**: Aggregate TAP receipts
- **Health Check**: Aggregator endpoint
- **Dependencies**: tap-contracts deployed

### 4. Application Services

#### tap-escrow-manager
- **Type**: Service (with initial Kafka setup)
- **Initial Task**: Create Kafka topic
- **Purpose**: Manage TAP escrow accounts
- **Health Check**: Manager endpoint
- **Dependencies**: redpanda, subgraph-deploy complete

#### gateway
- **Type**: Service
- **Purpose**: Route queries to indexers
- **Health Check**: Gateway endpoint
- **Dependencies**: indexer-service, tap-escrow-manager, subgraphs deployed
- **Restart Policy**: on-failure (up to 3 times)

#### block-explorer
- **Type**: Service (Optional)
- **Purpose**: Web UI for blockchain
- **Health Check**: HTTP endpoint
- **Dependencies**: anvil healthy

## Implementation Guidelines

### For Tasks

1. **Implement DeploymentTask trait**:
```rust
#[async_trait]
impl DeploymentTask for GraphContractsTask {
    async fn is_completed(&self) -> Result<bool> {
        // Check idempotency condition
    }
    
    async fn execute(&self, action: Action) -> Result<Receiver<Event>> {
        // Perform deployment/configuration
        // Exit when complete
    }
}
```

2. **Key Properties**:
   - Must check if already completed (idempotency)
   - Should verify success before exiting
   - Exit with code 0 on success
   - Can be re-run safely

### For Services

1. **Implement Service trait**:
```rust
#[async_trait]
impl Service for GraphNodeService {
    async fn dispatch_action(&self, action: Action) -> Result<Receiver<Event>> {
        // Handle service actions
    }
}

#[async_trait]
impl ServiceSetup for GraphNodeService {
    async fn is_setup_complete(&self) -> Result<bool> {
        // Check if initial setup is done
    }
    
    async fn perform_setup(&self) -> Result<()> {
        // Perform one-time setup if needed
    }
}
```

2. **Key Properties**:
   - Run continuously until stopped
   - Respond to health checks
   - Can be stopped and restarted
   - May have initial setup phase

## Dependency Resolution

### Task Dependencies
- Tasks can depend on:
  - Other tasks being completed
  - Services being healthy
- Example: `tap-contracts` depends on `graph-contracts` completion

### Service Dependencies  
- Services can depend on:
  - Other services being healthy
  - Tasks being completed
- Example: `graph-node` depends on `postgres`, `ipfs`, `anvil` being healthy

### Mixed Dependencies
- Example: `gateway` depends on:
  - Services: `indexer-service` healthy, `tap-escrow-manager` started
  - Tasks: `subgraph-deploy` completed

## State Machine Considerations

For complex task workflows, consider using state machines (e.g., with `statig` crate):

```rust
#[derive(State)]
enum DeploymentState {
    CheckingPrerequisites,
    DeployingContracts,
    VerifyingAddresses,
    DeployingSubgraph,
    Completed,
    Failed(String),
}
```

This provides:
- Clear state transitions
- Error recovery paths
- Progress tracking
- Idempotent operations