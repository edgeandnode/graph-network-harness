//! IPFS (InterPlanetary File System) service implementation
//!
//! This module provides the IPFS distributed storage service for the Graph Protocol stack.

use async_channel::Receiver;
use async_trait::async_trait;
use harness_core::config_traits::ServiceFromConfig;
use harness_core::{Error, prelude::*, service::Service};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use service_orchestration::{ServiceConfig, ServiceTarget};
use tracing::info;

/// IPFS (InterPlanetary File System) service
#[derive(Debug)]
pub struct IpfsService {
    api_port: u16,
    gateway_port: u16,
    event_tx: async_channel::Sender<IpfsEvent>,
    event_rx: async_channel::Receiver<IpfsEvent>,
}

impl IpfsService {
    /// Create a new IpfsService with specified API and gateway ports
    pub fn new(api_port: u16, gateway_port: u16) -> Self {
        let (event_tx, event_rx) = async_channel::unbounded();
        Self {
            api_port,
            gateway_port,
            event_tx,
            event_rx,
        }
    }
}

impl Default for IpfsService {
    fn default() -> Self {
        Self::new(5001, 8080)
    }
}

/// Actions for IPFS
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum IpfsAction {
    /// Check IPFS node status
    CheckStatus,
    /// Pin a hash to prevent garbage collection
    Pin {
        /// IPFS hash to pin
        hash: String,
    },
}

/// Events from IPFS
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event")]
pub enum IpfsEvent {
    /// Status check result
    StatusChecked {
        /// Whether the IPFS node is healthy
        healthy: bool,
        /// IPFS node version
        version: String,
        /// Number of connected peers
        peer_count: u32,
    },
    /// Hash pinned
    Pinned {
        /// IPFS hash that was pinned
        hash: String,
    },
    /// Action failed
    ActionFailed {
        /// Error message
        error: String,
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

    fn event_stream(&self) -> Receiver<Self::Event> {
        self.event_rx.clone()
    }

    async fn dispatch_action(&self, action: Self::Action) -> Result<(), Error> {
        let tx = self.event_tx.clone();
        let api_port = self.api_port;
        let gateway_port = self.gateway_port;

        // Spawn a task to handle the action
        let handle =
            smol::spawn(
                async move { handle_ipfs_action(action, tx, api_port, gateway_port).await },
            );

        // Detach the task so it runs in the background
        handle.detach();

        Ok(())
    }
}

async fn handle_ipfs_action(
    action: IpfsAction,
    _tx: async_channel::Sender<IpfsEvent>,
    _api_port: u16,
    _gateway_port: u16,
) -> Result<(), Error> {
    match action {
        IpfsAction::CheckStatus => {
            info!("Checking IPFS status");
            todo!("Implement actual status check via IPFS API")
        }

        IpfsAction::Pin { hash } => {
            info!("Pinning hash: {}", hash);
            todo!("Implement actual pinning via IPFS API")
        }
    }
}

/// ServiceSetup implementation for IpfsService
///
/// IPFS setup involves initializing the repository and configuring CORS for graph-node
#[async_trait]
impl ServiceSetup for IpfsService {
    async fn is_setup_complete(&self) -> Result<bool, Error> {
        info!(
            "Checking if IPFS setup is complete on API port {} and gateway port {}",
            self.api_port, self.gateway_port
        );

        // TODO: Implement actual setup check
        // This should check if:
        // 1. IPFS API is responding
        // 2. IPFS gateway is accessible
        // 3. CORS is properly configured for graph-node

        Ok(false)
    }

    async fn perform_setup(&self) -> Result<(), Error> {
        info!("Performing IPFS setup");

        // TODO: Implement actual setup
        // This should:
        // 1. Initialize IPFS repository if needed
        // 2. Configure CORS headers for graph-node access
        // 3. Configure API to listen on correct interface
        // 4. Start the IPFS daemon

        Ok(())
    }

    async fn validate_setup(&self) -> Result<(), Error> {
        info!("Validating IPFS setup");

        // TODO: Implement actual validation
        // This should verify:
        // 1. IPFS API is responding on expected port
        // 2. CORS headers are properly configured
        // 3. Can add and retrieve test content

        Ok(())
    }
}

impl ServiceFromConfig for IpfsService {
    fn from_config(config: &ServiceConfig) -> Result<Self, Error> {
        // Extract API and gateway ports from params
        let api_port = config
            .target
            .get_param_u16("api_port")
            .or_else(|| {
                if let ServiceTarget::Docker { ports, .. } = &config.target {
                    ports.get(0).cloned()
                } else {
                    None
                }
            })
            .unwrap_or(5001);

        let gateway_port = config
            .target
            .get_param_u16("gateway_port")
            .or_else(|| {
                if let ServiceTarget::Docker { ports, .. } = &config.target {
                    ports.get(1).cloned()
                } else {
                    None
                }
            })
            .unwrap_or(8080);

        Ok(IpfsService::new(api_port, gateway_port))
    }
}
