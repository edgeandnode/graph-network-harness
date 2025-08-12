//! Anvil blockchain service implementation
//!
//! This module provides the Anvil local blockchain service for testing.

use async_channel::Receiver;
use async_trait::async_trait;
use harness_core::config_traits::ServiceFromConfig;
use harness_core::{Error, prelude::*, service::Service};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use service_orchestration::{ServiceConfig, ServiceTarget};
use tracing::info;

/// Anvil blockchain service for testing
#[derive(Debug)]
pub struct AnvilService {
    chain_id: u64,
    port: u16,
    event_tx: async_channel::Sender<AnvilEvent>,
    event_rx: async_channel::Receiver<AnvilEvent>,
}

impl AnvilService {
    /// Create a new AnvilService with specified chain ID and port
    pub fn new(chain_id: u64, port: u16) -> Self {
        let (event_tx, event_rx) = async_channel::unbounded();
        Self {
            chain_id,
            port,
            event_tx,
            event_rx,
        }
    }
}

impl Default for AnvilService {
    fn default() -> Self {
        Self::new(31337, 8545)
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
        /// New balance in wei (as string to handle large numbers)
        balance: String,
    },
    /// Create a snapshot of the current state
    Snapshot,
    /// Revert to a snapshot
    RevertToSnapshot {
        /// Snapshot ID to revert to
        snapshot_id: String,
    },
    /// Deploy a contract
    DeployContract {
        /// Contract bytecode
        bytecode: String,
        /// Constructor arguments (ABI encoded)
        constructor_args: Option<String>,
    },
}

/// Events from Anvil blockchain
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event")]
pub enum AnvilEvent {
    /// Blocks were mined
    BlocksMined {
        /// Number of blocks mined
        count: u64,
        /// Latest block number
        latest_block: u64,
    },
    /// Balance was set
    BalanceSet {
        /// Address that was updated
        address: String,
        /// New balance
        balance: String,
    },
    /// Snapshot was created
    SnapshotCreated {
        /// ID of the snapshot
        snapshot_id: String,
    },
    /// Reverted to snapshot
    SnapshotReverted {
        /// ID of the snapshot reverted to
        snapshot_id: String,
    },
    /// Contract deployed
    ContractDeployed {
        /// Address of the deployed contract
        address: String,
        /// Transaction hash
        tx_hash: String,
    },
    /// Action failed
    ActionFailed {
        /// Error message
        error: String,
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
        "Anvil local Ethereum blockchain"
    }

    fn event_stream(&self) -> Receiver<Self::Event> {
        self.event_rx.clone()
    }

    async fn dispatch_action(&self, action: Self::Action) -> Result<(), Error> {
        let tx = self.event_tx.clone();
        let chain_id = self.chain_id;
        let port = self.port;

        todo!("handle dispatch_action in anvil service");

        Ok(())
    }
}

/// ServiceSetup implementation for AnvilService
///
/// Anvil requires minimal setup - just needs to start with the right chain configuration
#[async_trait]
impl ServiceSetup for AnvilService {
    async fn is_setup_complete(&self) -> Result<bool, Error> {
        info!(
            "Checking if Anvil setup is complete on port {} for chain {}",
            self.port, self.chain_id
        );

        // In a real implementation, this would check if Anvil is responding to RPC calls
        Ok(true)
    }

    async fn perform_setup(&self) -> Result<(), Error> {
        info!(
            "Performing Anvil setup for chain {} on port {}",
            self.chain_id, self.port
        );

        // Anvil setup might include:
        // 1. Deploying initial contracts
        // 2. Setting up test accounts with balances
        // 3. Mining initial blocks

        Ok(())
    }

    async fn validate_setup(&self) -> Result<(), Error> {
        info!("Validating Anvil setup");

        // In a real implementation, this would:
        // 1. Check RPC endpoint is responding
        // 2. Verify chain ID matches expected
        // 3. Check that expected accounts exist

        Ok(())
    }
}

impl ServiceFromConfig for AnvilService {
    fn from_config(config: &ServiceConfig) -> Result<Self, Error> {
        // Extract chain_id and port from params
        let chain_id = config
            .target
            .get_param_u64("chain_id")
            .or_else(|| {
                config
                    .target
                    .env()
                    .get("CHAIN_ID")
                    .and_then(|s| s.parse::<u64>().ok())
            })
            .unwrap_or(31337);

        let port = config
            .target
            .get_param_u16("port")
            .or_else(|| {
                // For Docker, check ports array
                if let ServiceTarget::Docker { ports, .. } = &config.target {
                    ports.first().cloned()
                } else {
                    None
                }
            })
            .unwrap_or(8545);

        Ok(AnvilService::new(chain_id, port))
    }
}
