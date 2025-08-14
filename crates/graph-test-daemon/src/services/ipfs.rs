//! IPFS (InterPlanetary File System) service implementation
//!
//! This module provides the IPFS distributed storage service for the Graph Protocol stack.

use async_channel::Receiver;
use async_trait::async_trait;
use harness_core::action::JsonAction;
use harness_core::config_traits::ServiceFromConfig;
use harness_core::{
    Error,
    service::{Service, ServiceEvents, ServiceSetup},
};
use harness_macros::{json_action, json_actions};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use service_orchestration::{ServiceConfig, ServiceTarget};
use std::result::Result;
use tracing::info;

/// IPFS (InterPlanetary File System) service
#[derive(Debug)]
pub struct IpfsService {
    api_port: u16,
    gateway_port: u16,
    event_tx: async_channel::Sender<IpfsEvent>,
    event_rx: async_channel::Receiver<IpfsEvent>,
}

#[json_actions]
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

    /// Check IPFS node status
    #[json_action]
    pub async fn check_status(&self) -> Result<IpfsStatusResult, Error> {
        info!("Checking IPFS status on API port {}", self.api_port);

        // In a real implementation, this would call IPFS API
        let status = IpfsStatusResult {
            online: true,
            peer_count: 5,
            repo_size_bytes: 1024 * 1024 * 100, // 100MB mock
        };

        // Emit event
        let _ = self
            .event_tx
            .send(IpfsEvent::StatusChecked {
                healthy: status.online,
                version: "0.15.0".to_string(),
                peer_count: status.peer_count as u32,
            })
            .await;

        Ok(status)
    }

    /// Pin a hash to IPFS
    #[json_action]
    pub async fn pin_hash(&self, hash: String) -> Result<(), Error> {
        info!("Pinning hash {} to IPFS", hash);

        // In a real implementation, this would call IPFS pin API

        // Emit event
        let _ = self
            .event_tx
            .send(IpfsEvent::Pinned { hash: hash.clone() })
            .await;

        Ok(())
    }

    /// Add data to IPFS and return the hash
    #[json_action]
    pub async fn add_data(&self, data: Vec<u8>) -> Result<String, Error> {
        info!("Adding {} bytes of data to IPFS", data.len());

        // In a real implementation, this would call IPFS add API
        // For now, generate a mock hash
        let hash = format!("Qm{}", uuid::Uuid::new_v4().to_string().replace("-", ""));

        // Emit event
        let _ = self
            .event_tx
            .send(IpfsEvent::Pinned { hash: hash.clone() })
            .await;

        Ok(hash)
    }

    /// Get data from IPFS by hash
    #[json_action]
    pub async fn get_data(&self, hash: String) -> Result<Vec<u8>, Error> {
        info!("Getting data from IPFS hash {}", hash);

        // In a real implementation, this would call IPFS cat API
        // For now, return mock data
        Ok(b"mock IPFS data".to_vec())
    }
}

impl Default for IpfsService {
    fn default() -> Self {
        Self::new(5001, 8080)
    }
}

/// Result of checking IPFS status
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct IpfsStatusResult {
    /// Whether the node is online
    pub online: bool,
    /// Number of connected peers
    pub peer_count: usize,
    /// Repository size in bytes
    pub repo_size_bytes: u64,
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

impl Service for IpfsService {
    const SERVICE_TYPE: &'static str = "ipfs";

    fn name(&self) -> &str {
        "ipfs"
    }

    fn description(&self) -> &str {
        "IPFS distributed storage service"
    }
}

#[async_trait]
impl ServiceEvents for IpfsService {
    type Event = IpfsEvent;

    fn event_stream(&self) -> Receiver<Self::Event> {
        self.event_rx.clone()
    }
}

/// ServiceSetup implementation for IpfsService
///
/// IPFS setup involves initializing the repository and configuring CORS for graph-node
#[async_trait]
impl ServiceSetup for IpfsService {
    async fn validate_setup(&self) -> Result<(), Error> {
        info!(
            "Validating IPFS setup on API port {} and gateway port {}",
            self.api_port, self.gateway_port
        );

        // In a real implementation, this would:
        // 1. Check IPFS API is responding
        // 2. Verify IPFS gateway is accessible
        // 3. Check CORS is properly configured for graph-node

        Ok(())
    }

    async fn perform_setup(&self) -> Result<(), Error> {
        info!("Performing IPFS setup");

        // In a real implementation, this would:
        // 1. Initialize IPFS repository if needed
        // 2. Configure CORS headers for graph-node access
        // 3. Configure API to listen on correct interface
        // 4. Start the IPFS daemon

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
                    ports.first().cloned()
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
