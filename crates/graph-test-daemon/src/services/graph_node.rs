//! Graph Node service implementation
//!
//! This module provides the Graph Node service that can deploy and manage subgraphs.

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
use service_orchestration::ServiceConfig;
use std::result::Result;
use tracing::info;

/// Graph Node service that can deploy and manage subgraphs
#[derive(Debug)]
pub struct GraphNodeService {
    endpoint: String,
    event_tx: async_channel::Sender<GraphNodeEvent>,
    event_rx: async_channel::Receiver<GraphNodeEvent>,
}

#[json_actions]
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

    /// Deploy a new subgraph
    #[json_action]
    pub async fn deploy_subgraph(
        &self,
        name: String,
        ipfs_hash: String,
        version_label: Option<String>,
    ) -> Result<DeploymentResult, Error> {
        info!("Deploying subgraph {} from IPFS hash {}", name, ipfs_hash);

        // In a real implementation, this would call Graph Node's admin API
        let deployment_id = format!("Qm{}_{}", &ipfs_hash[2..10], uuid::Uuid::new_v4());

        // Emit events
        let _ = self
            .event_tx
            .send(GraphNodeEvent::DeploymentStarted {
                deployment_id: deployment_id.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            })
            .await;

        // Simulate deployment progress
        let _ = self
            .event_tx
            .send(GraphNodeEvent::DeploymentProgress {
                deployment_id: deployment_id.clone(),
                status: "Syncing blocks".to_string(),
                percent: 50,
            })
            .await;

        let endpoints = vec![
            format!("http://{}:8000/subgraphs/name/{}", self.endpoint, name),
            format!("http://{}:8030/graphql", self.endpoint),
        ];

        let _ = self
            .event_tx
            .send(GraphNodeEvent::DeploymentCompleted {
                deployment_id: deployment_id.clone(),
                endpoints: endpoints.clone(),
            })
            .await;

        Ok(DeploymentResult {
            deployment_id,
            endpoints,
        })
    }

    /// Query a deployed subgraph
    #[json_action]
    pub async fn query_subgraph(
        &self,
        subgraph_name: String,
        query: String,
    ) -> Result<serde_json::Value, Error> {
        info!("Querying subgraph {} with query: {}", subgraph_name, query);

        // In a real implementation, this would send a GraphQL query to the subgraph
        let result = serde_json::json!({
            "data": {
                "example": "response"
            }
        });

        let _ = self
            .event_tx
            .send(GraphNodeEvent::QueryResult {
                data: result.clone(),
            })
            .await;

        Ok(result)
    }

    /// Remove a subgraph deployment
    #[json_action]
    pub async fn remove_subgraph(&self, deployment_id: String) -> Result<bool, Error> {
        info!("Removing subgraph deployment: {}", deployment_id);

        // In a real implementation, this would call Graph Node's admin API
        Ok(true)
    }
}

impl Default for GraphNodeService {
    fn default() -> Self {
        Self::new(String::new())
    }
}

/// Result of a deployment operation
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeploymentResult {
    /// ID of the deployment
    pub deployment_id: String,
    /// GraphQL endpoints for the deployed subgraph
    pub endpoints: Vec<String>,
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

impl Service for GraphNodeService {
    const SERVICE_TYPE: &'static str = "graph-node";

    fn name(&self) -> &str {
        "graph-node"
    }

    fn description(&self) -> &str {
        "Graph Protocol indexer node"
    }
}

#[async_trait]
impl ServiceEvents for GraphNodeService {
    type Event = GraphNodeEvent;

    fn event_stream(&self) -> Receiver<Self::Event> {
        self.event_rx.clone()
    }
}

/// ServiceSetup implementation for GraphNodeService
///
/// Graph Node doesn't require complex setup - it starts up and connects to dependencies.
/// Setup completion is determined by health check success.
#[async_trait]
impl ServiceSetup for GraphNodeService {
    async fn validate_setup(&self) -> Result<(), Error> {
        info!("Validating Graph Node setup at endpoint: {}", self.endpoint);

        // In a real implementation, this would:
        // 1. Check GraphQL endpoint is responding
        // 2. Verify database connection
        // 3. Check IPFS connectivity
        // 4. Ensure Ethereum RPC connection

        // For now, assume setup is valid if we can construct the service
        Ok(())
    }

    async fn perform_setup(&self) -> Result<(), Error> {
        info!("Performing Graph Node setup");

        // Graph Node setup is primarily handled by service orchestration
        // The main setup is ensuring database connections and IPFS connectivity
        // This would be where we'd verify connections and perform any initialization

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
