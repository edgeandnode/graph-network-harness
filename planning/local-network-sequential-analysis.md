# Local-Network Sequential Setup Analysis

## Service Startup Sequence

### Phase 1: Infrastructure Layer (No Dependencies)
These services can start in parallel:
1. **Chain (Anvil)** - Local Ethereum blockchain
2. **IPFS** - Distributed storage
3. **PostgreSQL** - Database with 3 schemas
4. **Redpanda** - Kafka-compatible message broker

### Phase 2: Core Services
1. **Graph Node** (depends on Chain, IPFS, Postgres healthy)
   - Pre-start: Mines one block to ensure chain has data
   - Creates connection to Ethereum RPC, IPFS, and Postgres
   - Exposes GraphQL, Admin, Status, and Metrics endpoints

### Phase 3: Contract Deployment
1. **Graph Contracts** (depends on Graph Node healthy)
   - Idempotency: Checks if graph-network subgraph exists, exits if yes
   - Deploys ~12 core protocol contracts using hardhat
   - Verifies deployed addresses match expected (contracts.json)
   - Deploys graph-network subgraph to graph-node
   - One-time operation

2. **TAP Contracts** (depends on Graph Contracts completed)
   - Idempotency: Checks if TAP subgraph exists
   - Deploys 3 TAP contracts using forge
   - Verifies addresses match expected
   - Deploys TAP subgraph
   - One-time operation

### Phase 4: Oracle and Agent Services
1. **Block Oracle** (depends on TAP Contracts completed)
   - Deploys EventfulDataEdge contract
   - Registers networks via contract call
   - Deploys block-oracle subgraph
   - Generates config.toml
   - Starts oracle service (long-running)

2. **Indexer Agent** (depends on Block Oracle healthy)
   - Checks if indexer is staked
   - If not: transfers ETH/GRT, approves, and stakes
   - Generates config.yaml and tap-contracts.json
   - Starts agent service (long-running)

### Phase 5: Deployment and Configuration
1. **Subgraph Deploy** (depends on Graph Node, Indexer Agent, IPFS healthy)
   - Idempotency: Checks if test subgraphs exist
   - Gets deployment IDs for network, block-oracle, TAP subgraphs
   - Forces indexing of core subgraphs
   - Publishes to GNS contract
   - Waits for active allocation (mining blocks if needed)
   - One-time operation

### Phase 6: Application Services
1. **TAP Escrow Manager** (depends on Redpanda healthy, Subgraph Deploy completed)
   - Creates Kafka topic
   - Generates config with contract addresses
   - Starts service (long-running)

2. **Indexer Service** (depends on Indexer Agent healthy, IPFS healthy, TAP Escrow Manager started)
   - Configures environment
   - Starts service (long-running)

3. **TAP Agent** (depends on Indexer Agent healthy)
   - Simple start with config (long-running)

4. **TAP Aggregator** (depends on TAP Contracts completed)
   - Simple start with config (long-running)

### Phase 7: Gateway
1. **Gateway** (depends on Subgraph Deploy completed, Indexer Service healthy, Redpanda healthy, TAP Escrow Manager started)
   - Gets network subgraph deployment ID
   - Generates config with trusted indexers
   - Starts gateway (long-running)
   - Has restart policy on failure

### Phase 8: Optional Services
1. **Block Explorer** (depends on Chain healthy)
   - Simple web UI (long-running)

## Key Sequential Patterns

### 1. Idempotency Checks
Many one-time services check if their work is already done:
```bash
# Check if subgraph exists
if curl -s http://graph-node:${GRAPH_NODE_GRAPHQL}/subgraphs/name/graph-network \
  -H 'content-type: application/json' \
  -d '{"query": "{ _meta { deployment } }" }' | grep "_meta"
then
  exit 0
fi
```

### 2. Contract Address Verification
After deploying contracts, services verify addresses match expected:
```bash
test "$(jq '."1337".Controller.address' /opt/contracts.json)" = \
     "$(jq '."1337".Controller.address' addresses-local.json)"
```

### 3. Dynamic Configuration Generation
Services generate config files using deployed contract addresses:
```bash
cat >config.yaml <<-EOF
networkIdentifier: "hardhat"
indexerOptions:
  geoCoordinates: [48.4682, -123.524]
  defaultAllocationAmount: 10000
EOF
```

### 4. Blockchain State Checks
Services query blockchain state before taking actions:
```bash
indexer_staked="$(cast call "--rpc-url=http://chain:${CHAIN_RPC}" \
  "${staking_address}" 'hasStake(address) (bool)' "${RECEIVER_ADDRESS}")"
if [ "${indexer_staked}" = "false" ]; then
  # Stake indexer
fi
```

### 5. Polling with Block Mining
Some services poll for conditions while mining blocks:
```bash
while ! curl -s "http://graph-node:${GRAPH_NODE_GRAPHQL}/subgraphs/name/graph-network" \
  -d '{"query": "{ allocations(where:{status:Active}) { indexer { id } } }" }' \
  | grep -i "${RECEIVER_ADDRESS}"
do
  cast rpc --rpc-url="http://chain:${CHAIN_RPC}" evm_mine
  sleep 2
done
```

### 6. Subgraph Deployment Pattern
```bash
# Create subgraph
npx graph create graph-network --node="http://graph-node:${GRAPH_NODE_ADMIN}"
# Deploy with version
npx graph deploy graph-network --node="http://graph-node:${GRAPH_NODE_ADMIN}" \
  --ipfs="http://ipfs:${IPFS_RPC}" --version-label=v0.0.1
# Extract deployment ID and reassign to node
deployment_id="$(grep "Build completed: " deploy.txt | awk '{print $3}')"
curl -s "http://graph-node:${GRAPH_NODE_ADMIN}" \
  -d "{\"jsonrpc\":\"2.0\",\"method\":\"subgraph_reassign\",\"params\":{\"ipfs_hash\":\"${deployment_id}\"}}"
```

## Critical Dependencies

### 1. contracts.json
- Pre-populated with expected contract addresses
- All services verify against this file
- Used for configuration generation

### 2. Environment Variables (.env)
- Shared across all services
- Contains ports, mnemonics, addresses
- Source'd at start of each script

### 3. Health Check Types
- HTTP endpoints (curl)
- Command execution (pg_isready, ipfs id, cast block)
- GraphQL queries for state verification

### 4. Dependency Conditions
- **service_healthy**: Wait for health check to pass
- **service_completed_successfully**: Wait for one-time service to exit 0
- **service_started**: Just wait for service to start

## Blocking Operations

1. **Contract Deployments** - Must complete before dependent services
2. **Subgraph Deployments** - Must sync before queries work
3. **Indexer Staking** - Must complete before allocations
4. **Active Allocation** - Gateway needs this to function
5. **Kafka Topic Creation** - Required for message passing

## Error Handling Patterns

1. **Idempotency** - Skip if already done
2. **Verification** - Test expected vs actual
3. **Retries** - Poll with sleep/mine loops
4. **Restart Policies** - Gateway has on-failure:3

## Key Insights for Harness Design

1. **Service Categories**:
   - Infrastructure (no deps, just health checks)
   - Deployment (one-time, idempotent, completion verification)
   - Application (long-running, complex deps, health checks)

2. **Dependency Types**:
   - Health-based (HTTP/command checks)
   - Completion-based (exit code 0)
   - State-based (blockchain/subgraph queries)

3. **Configuration Flow**:
   - Deploy contracts → Extract addresses → Generate configs → Start services

4. **Blockchain Integration**:
   - Cast commands for deployment and queries
   - Block mining to advance state
   - Transaction verification

5. **Subgraph Management**:
   - Create, deploy, get deployment ID, reassign to node
   - Query for state verification
   - Force indexing for critical subgraphs