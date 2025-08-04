//! Graph Protocol services using the new Service trait architecture
//!
//! This module defines services for Graph Protocol components that implement
//! the harness-core Service trait with strongly typed actions and events.

use async_channel::Receiver;
use async_trait::async_trait;
use harness_core::prelude::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::info;

/// Graph Node service that can deploy and manage subgraphs
#[derive(Default)]
pub struct GraphNodeService {
    endpoint: String,
}

impl GraphNodeService {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Actions that can be performed on a Graph Node
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum GraphNodeAction {
    /// Deploy a new subgraph
    DeploySubgraph {
        name: String,
        ipfs_hash: String,
        version_label: Option<String>,
    },
    /// Query a deployed subgraph
    QuerySubgraph {
        subgraph_name: String,
        query: String,
    },
    /// Remove a subgraph deployment
    RemoveSubgraph { deployment_id: String },
}

/// Events emitted by Graph Node actions
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event")]
pub enum GraphNodeEvent {
    /// Deployment started
    DeploymentStarted {
        deployment_id: String,
        timestamp: String,
    },
    /// Deployment progress
    DeploymentProgress {
        deployment_id: String,
        status: String,
        percent: u8,
    },
    /// Deployment completed
    DeploymentCompleted {
        deployment_id: String,
        endpoints: Vec<String>,
    },
    /// Query result
    QueryResult { data: serde_json::Value },
    /// Error occurred
    Error { message: String },
}

#[async_trait]
impl Service for GraphNodeService {
    type Action = GraphNodeAction;
    type Event = GraphNodeEvent;

    fn service_type() -> &'static str {
        "graph-node"
    }

    fn name(&self) -> &str {
        "graph-node"
    }

    fn description(&self) -> &str {
        "Graph Node service for subgraph deployment and querying"
    }

    async fn dispatch_action(&self, action: Self::Action) -> Result<Receiver<Self::Event>> {
        let (tx, rx) = async_channel::unbounded();

        match action {
            GraphNodeAction::DeploySubgraph {
                name,
                ipfs_hash,
                version_label,
            } => {
                info!(
                    "Deploying subgraph '{}' with IPFS hash '{}'",
                    name, ipfs_hash
                );

                // Send deployment started event
                let _ = tx
                    .send(GraphNodeEvent::DeploymentStarted {
                        deployment_id: format!("Qm{}", &ipfs_hash[2..]),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    })
                    .await;

                // Simulate deployment progress
                for i in 0..=100 {
                    if i % 20 == 0 {
                        let _ = tx
                            .send(GraphNodeEvent::DeploymentProgress {
                                deployment_id: format!("Qm{}", &ipfs_hash[2..]),
                                status: "Syncing blockchain data".to_string(),
                                percent: i,
                            })
                            .await;
                        smol::Timer::after(Duration::from_millis(100)).await;
                    }
                }

                // Send completion
                let _ = tx
                    .send(GraphNodeEvent::DeploymentCompleted {
                        deployment_id: format!("Qm{}", &ipfs_hash[2..]),
                        endpoints: vec![
                            format!("http://{}:8000/subgraphs/name/{}", self.endpoint, name),
                            format!("ws://{}:8001/subgraphs/name/{}", self.endpoint, name),
                        ],
                    })
                    .await;
            }

            GraphNodeAction::QuerySubgraph {
                subgraph_name,
                query,
            } => {
                info!(
                    "Querying subgraph '{}' with query: {}",
                    subgraph_name, query
                );

                // Simulate query execution
                let _ = tx
                    .send(GraphNodeEvent::QueryResult {
                        data: json!({
                            "data": {
                                "example": "result"
                            }
                        }),
                    })
                    .await;
            }

            GraphNodeAction::RemoveSubgraph { deployment_id } => {
                info!("Removing subgraph deployment '{}'", deployment_id);

                let _ = tx
                    .send(GraphNodeEvent::DeploymentCompleted {
                        deployment_id: deployment_id.clone(),
                        endpoints: vec![],
                    })
                    .await;
            }
        }

        Ok(rx)
    }
}

/// ServiceSetup implementation for GraphNodeService
///
/// Graph Node doesn't require complex setup - it starts up and connects to dependencies.
/// Setup completion is determined by health check success.
#[async_trait]
impl ServiceSetup for GraphNodeService {
    async fn is_setup_complete(&self) -> Result<bool> {
        // For Graph Node, setup is complete when the service is healthy
        // In a real implementation, this would check the GraphQL endpoint
        info!(
            "Checking if Graph Node setup is complete at endpoint: {}",
            self.endpoint
        );

        // Simulate health check - in reality this would query http://graph-node:8030
        // For now, assume setup is complete if we can construct the service
        Ok(true)
    }

    async fn perform_setup(&self) -> Result<()> {
        info!("Performing Graph Node setup");

        // Graph Node setup is primarily handled by service orchestration
        // The main setup is ensuring database connections and IPFS connectivity
        // This would be where we'd verify connections and perform any initialization

        Ok(())
    }

    async fn validate_setup(&self) -> Result<()> {
        info!("Validating Graph Node setup");

        // In a real implementation, this would:
        // 1. Check GraphQL endpoint is responding
        // 2. Verify database connection
        // 3. Check IPFS connectivity
        // 4. Ensure Ethereum RPC connection

        Ok(())
    }
}

/// Anvil blockchain service for testing
pub struct AnvilService {
    chain_id: u64,
    port: u16,
}

impl AnvilService {
    pub fn new(chain_id: u64, port: u16) -> Self {
        Self { chain_id, port }
    }
}

impl Default for AnvilService {
    fn default() -> Self {
        Self {
            chain_id: 31337,
            port: 8545,
        }
    }
}

/// Actions for Anvil blockchain
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum AnvilAction {
    /// Mine a number of blocks
    MineBlocks {
        count: u64,
        interval_secs: Option<u64>,
    },
    /// Set account balance
    SetBalance { address: String, balance: String },
    /// Create a fork
    Fork {
        url: String,
        block_number: Option<u64>,
    },
}

/// Events from Anvil blockchain
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event")]
pub enum AnvilEvent {
    /// Block mined
    BlockMined {
        block_number: u64,
        block_hash: String,
    },
    /// Balance updated
    BalanceUpdated {
        address: String,
        new_balance: String,
    },
    /// Fork created
    ForkCreated {
        fork_url: String,
        forked_at_block: u64,
    },
    /// Error occurred
    Error { message: String },
}

#[async_trait]
impl Service for AnvilService {
    type Action = AnvilAction;
    type Event = AnvilEvent;

    fn service_type() -> &'static str {
        "anvil"
    }

    fn name(&self) -> &str {
        "anvil"
    }

    fn description(&self) -> &str {
        "Anvil local Ethereum blockchain for testing"
    }

    async fn dispatch_action(&self, action: Self::Action) -> Result<Receiver<Self::Event>> {
        let (tx, rx) = async_channel::unbounded();

        match action {
            AnvilAction::MineBlocks {
                count,
                interval_secs,
            } => {
                info!("Mining {} blocks", count);

                let interval = interval_secs.unwrap_or(0);
                for i in 0..count {
                    let _ = tx
                        .send(AnvilEvent::BlockMined {
                            block_number: 1000 + i,
                            block_hash: format!("0x{:064x}", i),
                        })
                        .await;

                    if interval > 0 {
                        smol::Timer::after(Duration::from_secs(interval)).await;
                    }
                }
            }

            AnvilAction::SetBalance { address, balance } => {
                info!("Setting balance for {} to {}", address, balance);

                let _ = tx
                    .send(AnvilEvent::BalanceUpdated {
                        address: address.clone(),
                        new_balance: balance,
                    })
                    .await;
            }

            AnvilAction::Fork { url, block_number } => {
                info!("Creating fork from {} at block {:?}", url, block_number);

                let _ = tx
                    .send(AnvilEvent::ForkCreated {
                        fork_url: url,
                        forked_at_block: block_number.unwrap_or(0),
                    })
                    .await;
            }
        }

        Ok(rx)
    }
}

/// PostgreSQL database service
pub struct PostgresService {
    db_name: String,
    port: u16,
}

impl PostgresService {
    pub fn new(db_name: String, port: u16) -> Self {
        Self { db_name, port }
    }
}

impl Default for PostgresService {
    fn default() -> Self {
        Self {
            db_name: "graph-node".to_string(),
            port: 5432,
        }
    }
}

/// Actions for PostgreSQL
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum PostgresAction {
    /// Create a new database
    CreateDatabase { name: String },
    /// Run a SQL query
    ExecuteQuery { query: String },
    /// Backup the database
    Backup { backup_path: String },
}

/// Events from PostgreSQL
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event")]
pub enum PostgresEvent {
    /// Database created
    DatabaseCreated { name: String },
    /// Query executed
    QueryExecuted { rows_affected: u64 },
    /// Backup completed
    BackupCompleted { path: String, size_bytes: u64 },
    /// Error occurred
    Error { message: String },
}

#[async_trait]
impl Service for PostgresService {
    type Action = PostgresAction;
    type Event = PostgresEvent;

    fn service_type() -> &'static str {
        "postgres"
    }

    fn name(&self) -> &str {
        "postgres"
    }

    fn description(&self) -> &str {
        "PostgreSQL database service"
    }

    async fn dispatch_action(&self, action: Self::Action) -> Result<Receiver<Self::Event>> {
        let (tx, rx) = async_channel::unbounded();

        match action {
            PostgresAction::CreateDatabase { name } => {
                info!("Creating database '{}'", name);

                let _ = tx
                    .send(PostgresEvent::DatabaseCreated { name: name.clone() })
                    .await;
            }

            PostgresAction::ExecuteQuery { query } => {
                info!("Executing query: {}", query);

                let _ = tx
                    .send(PostgresEvent::QueryExecuted {
                        rows_affected: 42, // Simulated
                    })
                    .await;
            }

            PostgresAction::Backup { backup_path } => {
                info!("Creating backup at {}", backup_path);

                let _ = tx
                    .send(PostgresEvent::BackupCompleted {
                        path: backup_path,
                        size_bytes: 1024 * 1024, // 1MB simulated
                    })
                    .await;
            }
        }

        Ok(rx)
    }
}

/// IPFS (InterPlanetary File System) service
pub struct IpfsService {
    api_port: u16,
    gateway_port: u16,
}

impl IpfsService {
    pub fn new(api_port: u16, gateway_port: u16) -> Self {
        Self {
            api_port,
            gateway_port,
        }
    }
}

impl Default for IpfsService {
    fn default() -> Self {
        Self {
            api_port: 5001,
            gateway_port: 8080,
        }
    }
}

/// Actions for IPFS
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum IpfsAction {
    /// Add content to IPFS
    AddContent { content: String },
    /// Pin a hash
    Pin { hash: String },
    /// Unpin a hash
    Unpin { hash: String },
    /// Get content by hash
    Cat { hash: String },
}

/// Events from IPFS
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event")]
pub enum IpfsEvent {
    /// Content added
    ContentAdded { hash: String, size: u64 },
    /// Hash pinned
    Pinned { hash: String },
    /// Hash unpinned
    Unpinned { hash: String },
    /// Content retrieved
    ContentRetrieved { hash: String, content: String },
    /// Error occurred
    Error { message: String },
}

#[async_trait]
impl Service for IpfsService {
    type Action = IpfsAction;
    type Event = IpfsEvent;

    fn service_type() -> &'static str {
        "ipfs"
    }

    fn name(&self) -> &str {
        "ipfs"
    }

    fn description(&self) -> &str {
        "IPFS distributed storage service"
    }

    async fn dispatch_action(&self, action: Self::Action) -> Result<Receiver<Self::Event>> {
        let (tx, rx) = async_channel::unbounded();

        match action {
            IpfsAction::AddContent { content } => {
                info!("Adding content to IPFS");

                // Simulate IPFS hash generation
                let hash = format!("Qm{:x}", content.len());

                let _ = tx
                    .send(IpfsEvent::ContentAdded {
                        hash,
                        size: content.len() as u64,
                    })
                    .await;
            }

            IpfsAction::Pin { hash } => {
                info!("Pinning hash: {}", hash);

                let _ = tx.send(IpfsEvent::Pinned { hash: hash.clone() }).await;
            }

            IpfsAction::Unpin { hash } => {
                info!("Unpinning hash: {}", hash);

                let _ = tx.send(IpfsEvent::Unpinned { hash: hash.clone() }).await;
            }

            IpfsAction::Cat { hash } => {
                info!("Retrieving content for hash: {}", hash);

                let _ = tx
                    .send(IpfsEvent::ContentRetrieved {
                        hash: hash.clone(),
                        content: "Example content".to_string(),
                    })
                    .await;
            }
        }

        Ok(rx)
    }
}

/// Graph Contracts service that handles protocol contract deployment
///
/// This service implements the setup patterns from local-network/graph-contracts/run.sh
#[derive(Default)]
pub struct GraphContractsService {
    /// Contract addresses after deployment (None until setup is complete)
    deployed_addresses: Option<std::collections::HashMap<String, String>>,
}

impl GraphContractsService {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Actions for Graph Contracts service
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum GraphContractsAction {
    /// Deploy all Graph Protocol contracts
    DeployContracts,
    /// Verify deployed contract addresses
    VerifyAddresses,
    /// Get deployment status
    GetStatus,
}

/// Events from Graph Contracts service
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event")]
pub enum GraphContractsEvent {
    /// Contract deployment started
    DeploymentStarted,
    /// Contract deployed
    ContractDeployed { name: String, address: String },
    /// All contracts deployed
    DeploymentCompleted {
        addresses: std::collections::HashMap<String, String>,
    },
    /// Address verification completed
    AddressesVerified,
    /// Subgraph deployed
    SubgraphDeployed { deployment_id: String },
    /// Error occurred
    Error { message: String },
}

#[async_trait]
impl Service for GraphContractsService {
    type Action = GraphContractsAction;
    type Event = GraphContractsEvent;

    fn service_type() -> &'static str {
        "graph-contracts"
    }

    fn name(&self) -> &str {
        "graph-contracts"
    }

    fn description(&self) -> &str {
        "Graph Protocol contract deployment service"
    }

    async fn dispatch_action(&self, action: Self::Action) -> Result<Receiver<Self::Event>> {
        let (tx, rx) = async_channel::unbounded();

        match action {
            GraphContractsAction::DeployContracts => {
                info!("Starting Graph Protocol contract deployment");

                let _ = tx.send(GraphContractsEvent::DeploymentStarted).await;

                // Simulate deploying each contract (based on local-network script)
                let contracts = vec![
                    "Controller",
                    "EpochManager",
                    "GraphToken",
                    "DisputeManager",
                    "L1Staking",
                    "StakingExtension",
                    "Curation",
                    "RewardsManager",
                    "ServiceRegistry",
                    "L1GNS",
                    "SubgraphNFT",
                    "L1GraphTokenGateway",
                ];

                let mut addresses = std::collections::HashMap::new();
                for contract in contracts {
                    let address = format!("0x{:040x}", contract.len()); // Mock address
                    addresses.insert(contract.to_string(), address.clone());

                    let _ = tx
                        .send(GraphContractsEvent::ContractDeployed {
                            name: contract.to_string(),
                            address,
                        })
                        .await;

                    // Small delay to simulate deployment time
                    smol::Timer::after(Duration::from_millis(100)).await;
                }

                let _ = tx
                    .send(GraphContractsEvent::DeploymentCompleted {
                        addresses: addresses.clone(),
                    })
                    .await;

                // Simulate subgraph deployment
                let _ = tx
                    .send(GraphContractsEvent::SubgraphDeployed {
                        deployment_id: "QmGraphNetwork123".to_string(),
                    })
                    .await;
            }

            GraphContractsAction::VerifyAddresses => {
                info!("Verifying contract addresses");

                // In real implementation, this would verify against contracts.json
                let _ = tx.send(GraphContractsEvent::AddressesVerified).await;
            }

            GraphContractsAction::GetStatus => {
                info!("Getting deployment status");

                if let Some(addresses) = &self.deployed_addresses {
                    let _ = tx
                        .send(GraphContractsEvent::DeploymentCompleted {
                            addresses: addresses.clone(),
                        })
                        .await;
                }
            }
        }

        Ok(rx)
    }
}

/// ServiceSetup implementation for GraphContractsService
///
/// This implements the complex setup logic from local-network/graph-contracts/run.sh
#[async_trait]
impl ServiceSetup for GraphContractsService {
    async fn is_setup_complete(&self) -> Result<bool> {
        info!("Checking if Graph Protocol contracts are already deployed");

        // Check if graph-network subgraph exists (idempotency check from script)
        // In real implementation: curl http://graph-node:8030/subgraphs/name/graph-network

        // For now, check if we have deployed addresses
        Ok(self.deployed_addresses.is_some())
    }

    async fn perform_setup(&self) -> Result<()> {
        info!("Deploying Graph Protocol contracts and subgraph");

        // This would implement the script logic:
        // 1. Deploy contracts using hardhat
        // 2. Verify addresses match expected
        // 3. Build and deploy graph-network subgraph
        // 4. Store addresses for future reference

        Ok(())
    }

    async fn validate_setup(&self) -> Result<()> {
        info!("Validating Graph Protocol contract deployment");

        // This would verify:
        // 1. All contract addresses match expected values in contracts.json
        // 2. Graph-network subgraph is deployed and healthy
        // 3. Contracts are accessible and responding

        Ok(())
    }
}

/// Stack enum containing all available services
pub enum GraphTestStack {
    GraphNode(GraphNodeService),
    Anvil(AnvilService),
    Postgres(PostgresService),
    Ipfs(IpfsService),
    GraphContracts(GraphContractsService),
}
