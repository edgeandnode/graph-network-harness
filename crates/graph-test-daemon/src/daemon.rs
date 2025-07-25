//! Graph Test Daemon implementation
//!
//! This module implements the GraphTestDaemon which extends BaseDaemon with
//! Graph Protocol specific functionality for automated integration testing.

use async_trait::async_trait;
use harness_core::prelude::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use tracing::{info, warn, error};

use crate::actions::*;
use crate::services::*;

/// Graph Protocol specialized testing daemon
pub struct GraphTestDaemon {
    /// Base daemon functionality
    base: BaseDaemon,
    /// Graph-specific state
    graph_state: GraphState,
}

/// Graph Protocol specific state
#[derive(Debug, Default)]
pub struct GraphState {
    /// Deployed subgraphs
    pub subgraphs: HashMap<String, SubgraphInfo>,
    /// Blockchain state
    pub blockchain: BlockchainState,
    /// Running indexer allocations
    pub allocations: HashMap<String, AllocationInfo>,
}

/// Information about a deployed subgraph
#[derive(Debug, Clone)]
pub struct SubgraphInfo {
    /// Subgraph deployment ID
    pub deployment_id: String,
    /// IPFS hash of the subgraph manifest
    pub ipfs_hash: String,
    /// Current indexing status
    pub status: IndexingStatus,
    /// Block number when deployed
    pub deployed_at_block: u64,
}

/// Blockchain state tracking
#[derive(Debug, Default)]
pub struct BlockchainState {
    /// Current block number
    pub current_block: u64,
    /// Known contract addresses
    pub contracts: HashMap<String, String>,
    /// Pending transactions
    pub pending_txs: Vec<String>,
}

/// Indexing status of a subgraph
#[derive(Debug, Clone)]
pub enum IndexingStatus {
    /// Not yet started
    Pending,
    /// Currently syncing
    Syncing { current_block: u64, target_block: u64 },
    /// Fully synced
    Synced,
    /// Failed with error
    Failed(String),
}

/// Allocation information for indexing rewards
#[derive(Debug, Clone)]
pub struct AllocationInfo {
    /// Allocation ID
    pub id: String,
    /// Subgraph deployment being allocated to
    pub deployment_id: String,
    /// Allocated tokens amount
    pub tokens: String,
    /// Allocation creation block
    pub created_at_block: u64,
}

impl GraphTestDaemon {
    /// Create a new Graph Test Daemon
    pub async fn new(endpoint: SocketAddr) -> Result<Self> {
        let base = BaseDaemon::builder()
            .with_endpoint(endpoint)
            .with_registry_path("./graph-test-registry.db")
            // Register Graph Protocol service types
            .register_service_type::<AnvilBlockchain>()?
            .register_service_type::<IpfsNode>()?
            .register_service_type::<PostgresDb>()?
            .register_service_type::<GraphNode>()?
            // Register Graph Protocol actions
            .register_action(
                "deploy-subgraph",
                "Deploy a subgraph to the Graph Node",
                Self::deploy_subgraph_action,
            )?
            .register_action(
                "start-indexing",
                "Start indexing a subgraph",
                Self::start_indexing_action,
            )?
            .register_action(
                "mine-blocks",
                "Mine blocks on the test blockchain",
                Self::mine_blocks_action,
            )?
            .register_action(
                "create-allocation",
                "Create an indexer allocation",
                Self::create_allocation_action,
            )?
            .register_action(
                "trigger-reorg",
                "Trigger a blockchain reorganization for testing",
                Self::trigger_reorg_action,
            )?
            .register_action(
                "setup-stack",
                "Set up the complete Graph Protocol testing stack",
                Self::setup_stack_action,
            )?
            .register_action(
                "query-subgraph",
                "Execute a GraphQL query against a subgraph",
                Self::query_subgraph_action,
            )?
            .register_action(
                "wait-for-sync",
                "Wait for a subgraph to sync to a specific block",
                Self::wait_for_sync_action,
            )?
            .build()
            .await?;
        
        Ok(Self {
            base,
            graph_state: GraphState::default(),
        })
    }
    
    /// Get the current graph state
    pub fn graph_state(&self) -> &GraphState {
        &self.graph_state
    }
    
    /// Update blockchain state with new block
    pub fn update_block(&mut self, block_number: u64) {
        self.graph_state.blockchain.current_block = block_number;
    }
    
    /// Add a deployed subgraph to state
    pub fn add_subgraph(&mut self, name: String, info: SubgraphInfo) {
        self.graph_state.subgraphs.insert(name, info);
    }
    
    /// Add a contract address to state
    pub fn add_contract(&mut self, name: String, address: String) {
        self.graph_state.blockchain.contracts.insert(name, address);
    }
}

#[async_trait]
impl Action for GraphTestDaemon {
    fn actions(&self) -> &ActionRegistry {
        self.base.actions()
    }
    
    fn actions_mut(&mut self) -> &mut ActionRegistry {
        self.base.actions_mut()
    }
}

#[async_trait]
impl Daemon for GraphTestDaemon {
    async fn start(&self) -> Result<()> {
        info!("Starting Graph Test Daemon");
        self.base.start().await
    }
    
    async fn stop(&self) -> Result<()> {
        info!("Stopping Graph Test Daemon");
        self.base.stop().await
    }
    
    fn endpoint(&self) -> SocketAddr {
        self.base.endpoint()
    }
    
    fn service_manager(&self) -> &service_orchestration::ServiceManager {
        self.base.service_manager()
    }
    
    fn service_registry(&self) -> &service_registry::Registry {
        self.base.service_registry()
    }
}

// Action implementations will be in the actions module
impl GraphTestDaemon {
    async fn deploy_subgraph_action(params: Value) -> Result<Value> {
        DeploySubgraphAction::execute(params).await
    }
    
    async fn start_indexing_action(params: Value) -> Result<Value> {
        StartIndexingAction::execute(params).await
    }
    
    async fn mine_blocks_action(params: Value) -> Result<Value> {
        MineBlocksAction::execute(params).await
    }
    
    async fn create_allocation_action(params: Value) -> Result<Value> {
        CreateAllocationAction::execute(params).await
    }
    
    async fn trigger_reorg_action(params: Value) -> Result<Value> {
        TriggerReorgAction::execute(params).await
    }
    
    async fn setup_stack_action(params: Value) -> Result<Value> {
        SetupStackAction::execute(params).await
    }
    
    async fn query_subgraph_action(params: Value) -> Result<Value> {
        QuerySubgraphAction::execute(params).await
    }
    
    async fn wait_for_sync_action(params: Value) -> Result<Value> {
        WaitForSyncAction::execute(params).await
    }
}