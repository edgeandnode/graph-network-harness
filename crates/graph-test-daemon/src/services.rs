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
#[derive(Debug, Default)]
pub struct GraphNodeService {
    endpoint: String,
}

impl GraphNodeService {
    /// Create a new GraphNodeService with specified endpoint
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
        /// Name of the subgraph
        name: String,
        /// IPFS hash of the subgraph manifest
        ipfs_hash: String,
        /// Optional version label for the deployment
        version_label: Option<String>,
    },
    /// Query a deployed subgraph
    QuerySubgraph {
        /// Name of the subgraph to query
        subgraph_name: String,
        /// GraphQL query string
        query: String,
    },
    /// Remove a subgraph deployment
    RemoveSubgraph {
        /// ID of the deployment to remove
        deployment_id: String,
    },
}

/// Events emitted by Graph Node actions
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event")]
pub enum GraphNodeEvent {
    /// Deployment started
    DeploymentStarted {
        /// ID of the deployment
        deployment_id: String,
        /// Timestamp when deployment started
        timestamp: String,
    },
    /// Deployment progress
    DeploymentProgress {
        /// ID of the deployment
        deployment_id: String,
        /// Current status message
        status: String,
        /// Progress percentage (0-100)
        percent: u8,
    },
    /// Deployment completed
    DeploymentCompleted {
        /// ID of the deployment
        deployment_id: String,
        /// List of GraphQL endpoints for the deployed subgraph
        endpoints: Vec<String>,
    },
    /// Query result
    QueryResult {
        /// Query result data
        data: serde_json::Value,
    },
    /// Error occurred
    Error {
        /// Error message
        message: String,
    },
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
#[derive(Debug)]
pub struct AnvilService {
    chain_id: u64,
    port: u16,
}

impl AnvilService {
    /// Create a new AnvilService with specified chain ID and port
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
        /// Number of blocks to mine
        count: u64,
        /// Optional interval between blocks in seconds
        interval_secs: Option<u64>,
    },
    /// Set account balance
    SetBalance {
        /// Ethereum address
        address: String,
        /// New balance in wei
        balance: String,
    },
    /// Create a fork
    Fork {
        /// URL of the network to fork
        url: String,
        /// Optional block number to fork from
        block_number: Option<u64>,
    },
}

/// Events from Anvil blockchain
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event")]
pub enum AnvilEvent {
    /// Block mined
    BlockMined {
        /// Block number that was mined
        block_number: u64,
        /// Hash of the mined block
        block_hash: String,
    },
    /// Balance updated
    BalanceUpdated {
        /// Ethereum address that was updated
        address: String,
        /// New balance in wei
        new_balance: String,
    },
    /// Fork created
    ForkCreated {
        /// URL of the forked network
        fork_url: String,
        /// Block number where fork was created
        forked_at_block: u64,
    },
    /// Error occurred
    Error {
        /// Error message
        message: String,
    },
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

/// ServiceSetup implementation for AnvilService
///
/// Anvil is a foundation service that doesn't require complex setup - it starts immediately
#[async_trait]
impl ServiceSetup for AnvilService {
    async fn is_setup_complete(&self) -> Result<bool> {
        info!(
            "Checking if Anvil blockchain is ready on port {}",
            self.port
        );

        // Check if Anvil RPC endpoint is responding using TCP connection
        // For a real implementation, we'd send an RPC request, but for now
        // we'll just check if the port is open
        use std::net::TcpStream;
        use std::time::Duration;

        let addr = format!("127.0.0.1:{}", self.port);
        match TcpStream::connect_timeout(
            &addr.parse::<std::net::SocketAddr>().unwrap(),
            Duration::from_secs(1),
        ) {
            Ok(_) => {
                info!("Anvil is accepting connections on port {}", self.port);
                // In a real implementation, we'd send an eth_chainId RPC request
                // For now, assume if the port is open, Anvil is ready
                Ok(true)
            }
            Err(e) => {
                info!("Anvil not yet ready on port {}: {}", self.port, e);
                Ok(false)
            }
        }
    }

    async fn perform_setup(&self) -> Result<()> {
        info!("Performing Anvil setup (blockchain initialization)");

        // Anvil setup would involve:
        // 1. Starting the blockchain process
        // 2. Waiting for RPC to be available
        // 3. Pre-funding test accounts
        // For now, this is a no-op since we assume Anvil is started externally

        Ok(())
    }

    async fn validate_setup(&self) -> Result<()> {
        info!("Validating Anvil setup");

        // In a real implementation, this would:
        // 1. Check if RPC is responding on the expected port
        // 2. Verify test accounts are funded
        // 3. Ensure chain ID matches expected value

        Ok(())
    }
}

/// PostgreSQL database service
#[derive(Debug)]
pub struct PostgresService {
    db_name: String,
    port: u16,
}

impl PostgresService {
    /// Create a new PostgresService with specified database name and port
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
    CreateDatabase {
        /// Name of the database to create
        name: String,
    },
    /// Run a SQL query
    ExecuteQuery {
        /// SQL query to execute
        query: String,
    },
    /// Backup the database
    Backup {
        /// Path where backup should be saved
        backup_path: String,
    },
}

/// Events from PostgreSQL
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event")]
pub enum PostgresEvent {
    /// Database created
    DatabaseCreated {
        /// Name of the created database
        name: String,
    },
    /// Query executed
    QueryExecuted {
        /// Number of rows affected by the query
        rows_affected: u64,
    },
    /// Backup completed
    BackupCompleted {
        /// Path where backup was saved
        path: String,
        /// Size of the backup file in bytes
        size_bytes: u64,
    },
    /// Error occurred
    Error {
        /// Error message
        message: String,
    },
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
#[derive(Debug)]
pub struct IpfsService {
    api_port: u16,
    gateway_port: u16,
}

impl IpfsService {
    /// Create a new IpfsService with specified API and gateway ports
    pub fn new(api_port: u16, gateway_port: u16) -> Self {
        Self {
            api_port,
            gateway_port,
        }
    }
}

/// ServiceSetup implementation for PostgresService
///
/// PostgreSQL setup involves ensuring the database exists and has proper permissions
#[async_trait]
impl ServiceSetup for PostgresService {
    async fn is_setup_complete(&self) -> Result<bool> {
        info!(
            "Checking if PostgreSQL setup is complete for database '{}' on port {}",
            self.db_name, self.port
        );

        // For PostgreSQL, we need to check if:
        // 1. The server is accepting connections
        // 2. The graph-node database exists
        // Since we can't directly connect to PostgreSQL from here without a driver,
        // we'll use a simple TCP connection check for now
        use std::net::TcpStream;
        use std::time::Duration;

        let addr = format!("127.0.0.1:{}", self.port);
        match TcpStream::connect_timeout(
            &addr.parse::<std::net::SocketAddr>().unwrap(),
            Duration::from_secs(1),
        ) {
            Ok(_) => {
                info!("PostgreSQL is accepting connections on port {}", self.port);
                // In a real implementation, we'd also check if the database exists
                // For now, assume if PostgreSQL is up, setup can proceed
                Ok(true)
            }
            Err(e) => {
                info!("PostgreSQL not ready on port {}: {}", self.port, e);
                Ok(false)
            }
        }
    }

    async fn perform_setup(&self) -> Result<()> {
        info!(
            "Performing PostgreSQL setup for database '{}'",
            self.db_name
        );

        // In local-network, the postgres run.sh script:
        // 1. Creates the graph-node database if it doesn't exist
        // 2. Sets up the graph user with proper permissions
        // 3. Ensures the database is ready for graph-node to connect

        // This would execute:
        // CREATE DATABASE IF NOT EXISTS graph_node;
        // CREATE USER IF NOT EXISTS graph WITH PASSWORD 'let-me-in';
        // GRANT ALL PRIVILEGES ON DATABASE graph_node TO graph;

        Ok(())
    }

    async fn validate_setup(&self) -> Result<()> {
        info!("Validating PostgreSQL setup");

        // Validate that:
        // 1. Database exists and is accessible
        // 2. Required extensions are installed (if any)
        // 3. Connection parameters are correct

        Ok(())
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
    AddContent {
        /// Content to add to IPFS
        content: String,
    },
    /// Pin a hash
    Pin {
        /// IPFS hash to pin
        hash: String,
    },
    /// Unpin a hash
    Unpin {
        /// IPFS hash to unpin
        hash: String,
    },
    /// Get content by hash
    Cat {
        /// IPFS hash to retrieve
        hash: String,
    },
}

/// Events from IPFS
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event")]
pub enum IpfsEvent {
    /// Content added
    ContentAdded {
        /// IPFS hash of the added content
        hash: String,
        /// Size of the content in bytes
        size: u64,
    },
    /// Hash pinned
    Pinned {
        /// IPFS hash that was pinned
        hash: String,
    },
    /// Hash unpinned
    Unpinned {
        /// IPFS hash that was unpinned
        hash: String,
    },
    /// Content retrieved
    ContentRetrieved {
        /// IPFS hash that was retrieved
        hash: String,
        /// Retrieved content
        content: String,
    },
    /// Error occurred
    Error {
        /// Error message
        message: String,
    },
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

/// ServiceSetup implementation for IpfsService
///
/// IPFS setup involves initializing the repository and configuring CORS for graph-node
#[async_trait]
impl ServiceSetup for IpfsService {
    async fn is_setup_complete(&self) -> Result<bool> {
        info!(
            "Checking if IPFS setup is complete on API port {} and gateway port {}",
            self.api_port, self.gateway_port
        );

        // Check if IPFS API and gateway are responding using TCP connections
        use std::net::TcpStream;
        use std::time::Duration;

        // Check API port
        let api_addr = format!("127.0.0.1:{}", self.api_port);
        match TcpStream::connect_timeout(
            &api_addr.parse::<std::net::SocketAddr>().unwrap(),
            Duration::from_secs(1),
        ) {
            Ok(_) => {
                info!(
                    "IPFS API is accepting connections on port {}",
                    self.api_port
                );

                // Also check the gateway
                let gateway_addr = format!("127.0.0.1:{}", self.gateway_port);
                match TcpStream::connect_timeout(
                    &gateway_addr.parse::<std::net::SocketAddr>().unwrap(),
                    Duration::from_secs(1),
                ) {
                    Ok(_) => {
                        info!(
                            "IPFS gateway is accepting connections on port {}",
                            self.gateway_port
                        );
                        // In a real implementation, we'd send API requests to verify IPFS is working
                        Ok(true)
                    }
                    Err(e) => {
                        info!(
                            "IPFS gateway not ready on port {}: {}",
                            self.gateway_port, e
                        );
                        Ok(false)
                    }
                }
            }
            Err(e) => {
                info!("IPFS API not ready on port {}: {}", self.api_port, e);
                Ok(false)
            }
        }
    }

    async fn perform_setup(&self) -> Result<()> {
        info!("Performing IPFS setup");

        // In local-network, the IPFS run.sh script:
        // 1. Initializes IPFS if not already done (ipfs init)
        // 2. Configures CORS headers for graph-node access:
        //    ipfs config --json API.HTTPHeaders.Access-Control-Allow-Origin '["*"]'
        //    ipfs config --json API.HTTPHeaders.Access-Control-Allow-Methods '["GET", "POST"]'
        // 3. Configures API address to listen on all interfaces
        //    ipfs config Addresses.API /ip4/0.0.0.0/tcp/5001
        // 4. Starts the IPFS daemon

        Ok(())
    }

    async fn validate_setup(&self) -> Result<()> {
        info!("Validating IPFS setup");

        // Validate that:
        // 1. IPFS API is responding on expected port
        // 2. CORS headers are properly configured
        // 3. Can add and retrieve test content

        Ok(())
    }
}

/// Stack enum containing all available services
pub enum GraphTestStack {
    /// Graph Node service instance
    GraphNode(GraphNodeService),
    /// Anvil blockchain service instance
    Anvil(AnvilService),
    /// PostgreSQL database service instance
    Postgres(PostgresService),
    /// IPFS service instance
    Ipfs(IpfsService),
}

#[cfg(test)]
mod tests {
    // Add tests for remaining services as needed
}
