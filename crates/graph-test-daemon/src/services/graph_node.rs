//! Graph Node service implementation
//!
//! This module provides the Graph Node service that can deploy and manage subgraphs.

use async_channel::Receiver;
use async_trait::async_trait;
use harness_core::config_traits::ServiceFromConfig;
use harness_core::{Error, prelude::*, service::Service};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use service_orchestration::ServiceConfig;
use tracing::info;

/// Graph Node service that can deploy and manage subgraphs
#[derive(Debug)]
pub struct GraphNodeService {
    endpoint: String,
    event_tx: async_channel::Sender<GraphNodeEvent>,
    event_rx: async_channel::Receiver<GraphNodeEvent>,
}

impl GraphNodeService {
    /// Create a new GraphNodeService with specified endpoint
    pub fn new(endpoint: String) -> Self {
        let (event_tx, event_rx) = async_channel::unbounded();
        Self {
            endpoint,
            event_tx,
            event_rx,
        }
    }
}

impl Default for GraphNodeService {
    fn default() -> Self {
        Self::new(String::new())
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
    /// Action failed
    ActionFailed {
        /// Error message
        error: String,
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
        "Graph Protocol indexer node"
    }

    fn event_stream(&self) -> Receiver<Self::Event> {
        self.event_rx.clone()
    }

    async fn dispatch_action(&self, action: Self::Action) -> Result<(), Error> {
        let tx = self.event_tx.clone();
        let endpoint = self.endpoint.clone();

        todo!("impl graph-node dispatch_action");

        Ok(())
    }
}

/// ServiceSetup implementation for GraphNodeService
///
/// Graph Node doesn't require complex setup - it starts up and connects to dependencies.
/// Setup completion is determined by health check success.
#[async_trait]
impl ServiceSetup for GraphNodeService {
    async fn is_setup_complete(&self) -> Result<bool, Error> {
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

    async fn perform_setup(&self) -> Result<(), Error> {
        info!("Performing Graph Node setup");

        // Graph Node setup is primarily handled by service orchestration
        // The main setup is ensuring database connections and IPFS connectivity
        // This would be where we'd verify connections and perform any initialization

        Ok(())
    }

    async fn validate_setup(&self) -> Result<(), Error> {
        info!("Validating Graph Node setup");

        // In a real implementation, this would:
        // 1. Check GraphQL endpoint is responding
        // 2. Verify database connection
        // 3. Check IPFS connectivity
        // 4. Ensure Ethereum RPC connection

        Ok(())
    }
}

impl ServiceFromConfig for GraphNodeService {
    fn from_config(config: &ServiceConfig) -> Result<Self, Error> {
        // Extract endpoint from params or environment
        let endpoint = config
            .target
            .get_param_str("endpoint")
            .or_else(|| config.target.env().get("GRAPH_ENDPOINT").cloned())
            .unwrap_or_else(|| "localhost".to_string());

        Ok(GraphNodeService::new(endpoint))
    }
}
