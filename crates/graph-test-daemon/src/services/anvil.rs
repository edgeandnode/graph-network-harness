//! Anvil blockchain service implementation
//!
//! This module provides the Anvil local blockchain service for testing.

use async_channel::Receiver;
use async_trait::async_trait;
use harness_core::action::JsonAction;
use harness_core::config_traits::ServiceFromConfig;
use harness_core::{
    Error,
    service::{Service, ServiceEvents},
};
use harness_macros::{json_action, json_actions};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use service_orchestration::{ServiceConfig, ServiceTarget};
use std::result::Result;
use tracing::info;

/// Anvil blockchain service for testing
#[derive(Debug)]
pub struct AnvilService {
    chain_id: u64,
    port: u16,
    event_tx: async_channel::Sender<AnvilEvent>,
    event_rx: async_channel::Receiver<AnvilEvent>,
}

#[json_actions]
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

    /// Mine blocks on the blockchain
    #[json_action]
    pub async fn mine_blocks(&self, count: u64) -> Result<Vec<String>, Error> {
        info!("Mining {} blocks on chain {}", count, self.chain_id);

        // In a real implementation, this would call Anvil's RPC
        // For now, generate mock block hashes
        let mut block_hashes = Vec::new();
        for i in 0..count {
            block_hashes.push(format!("0x{:064x}", i));
        }

        // Emit event
        let _ = self
            .event_tx
            .send(AnvilEvent::BlocksMined {
                count,
                latest_block: 100 + count, // Mock block number
            })
            .await;

        Ok(block_hashes)
    }

    /// Set the balance of an address
    #[json_action]
    pub async fn set_balance(&self, address: String, balance: String) -> Result<(), Error> {
        info!("Setting balance for {} to {} wei", address, balance);

        // In a real implementation, this would call Anvil's RPC

        // Emit event
        let _ = self
            .event_tx
            .send(AnvilEvent::BalanceSet {
                address: address.clone(),
                balance: balance.clone(),
            })
            .await;

        Ok(())
    }

    /// Get the current block number
    #[json_action]
    pub async fn get_block_number(&self) -> Result<u64, Error> {
        info!("Getting current block number");

        // In a real implementation, this would call Anvil's RPC
        Ok(100) // Mock block number
    }

    /// Create a snapshot of the blockchain state
    #[json_action]
    pub async fn create_snapshot(&self) -> Result<String, Error> {
        info!("Creating blockchain snapshot");

        // In a real implementation, this would call Anvil's RPC
        let snapshot_id = format!("snapshot_{}", uuid::Uuid::new_v4());

        // Emit event
        let _ = self
            .event_tx
            .send(AnvilEvent::SnapshotCreated {
                snapshot_id: snapshot_id.clone(),
            })
            .await;

        Ok(snapshot_id)
    }

    /// Revert to a previous snapshot
    #[json_action]
    pub async fn revert_to_snapshot(&self, snapshot_id: String) -> Result<bool, Error> {
        info!("Reverting to snapshot: {}", snapshot_id);

        // In a real implementation, this would call Anvil's RPC

        // Emit event
        let _ = self
            .event_tx
            .send(AnvilEvent::SnapshotReverted {
                snapshot_id: snapshot_id.clone(),
            })
            .await;

        Ok(true)
    }
}

impl Default for AnvilService {
    fn default() -> Self {
        Self::new(31337, 8545)
    }
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

impl Service for AnvilService {
    const SERVICE_TYPE: &'static str = "anvil";

    fn name(&self) -> &str {
        "anvil"
    }

    fn description(&self) -> &str {
        "Anvil local Ethereum blockchain"
    }
}

#[async_trait]
impl ServiceEvents for AnvilService {
    type Event = AnvilEvent;

    fn event_stream(&self) -> Receiver<Self::Event> {
        self.event_rx.clone()
    }
}

/// ServiceSetup implementation for AnvilService
///
/// Anvil requires minimal setup - just needs to start with the right chain configuration
#[async_trait]
impl harness_core::service::ServiceSetup for AnvilService {
    async fn validate_setup(&self) -> Result<(), Error> {
        info!(
            "Validating Anvil setup on port {} for chain {}",
            self.port, self.chain_id
        );

        // In a real implementation, this would:
        // 1. Check RPC endpoint is responding
        // 2. Verify chain ID matches expected
        // 3. Check that expected accounts exist

        Ok(())
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
                // Check allocated_ports first (populated at runtime)
                config.allocated_ports.get("rpc").copied()
            })
            .unwrap_or(8545);

        Ok(AnvilService::new(chain_id, port))
    }
}
