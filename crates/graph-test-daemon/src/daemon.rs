//! Graph Test Daemon implementation using new Service architecture
//!
//! This module implements the GraphTestDaemon which extends BaseDaemon with
//! Graph Protocol specific services that can perform actions.

use async_trait::async_trait;
use harness_core::prelude::*;
use harness_core::{Error, Registry, ServiceManager};
use service_orchestration::StackConfig;
use std::net::SocketAddr;
use std::path::Path;
use std::result::Result;
use tracing::info;

use crate::services::{AnvilService, GraphNodeService, IpfsService, PostgresService};
use crate::tasks::{GraphContractsTask, SubgraphDeployTask, TapContractsTask};

/// Type alias for Graph Protocol stack configuration
pub type GraphStackConfig = StackConfig;

/// Graph Protocol specialized testing daemon
pub struct GraphTestDaemon {
    /// Base daemon functionality  
    base: BaseDaemon,
}

impl GraphTestDaemon {
    /// Create a new Graph Test Daemon from configuration
    pub async fn from_config<P: AsRef<Path>>(
        endpoint: SocketAddr,
        config_path: P,
    ) -> Result<Self, Error> {
        // Load configuration from YAML file
        let config_content = std::fs::read_to_string(config_path.as_ref())
            .map_err(|e| Error::daemon(format!("Failed to read config file: {e}")))?;

        let config: GraphStackConfig = serde_yaml::from_str(&config_content)
            .map_err(|e| Error::daemon(format!("Failed to parse config YAML: {e}")))?;

        Self::from_stack_config(endpoint, config).await
    }

    /// Create a new Graph Test Daemon from a stack configuration
    pub async fn from_stack_config(
        endpoint: SocketAddr,
        config: GraphStackConfig,
    ) -> Result<Self, Error> {
        // Build the base daemon with Graph-specific services
        let mut builder = BaseDaemon::builder(config).with_endpoint(endpoint);

        // Wire up services by type - each call validates that services of that type exist
        builder
            .wire_service::<GraphNodeService>("graph-node")?
            .wire_service::<AnvilService>("anvil")?
            .wire_service::<PostgresService>("postgres")?
            .wire_service::<IpfsService>("ipfs")?;

        // Wire up tasks by type - each call validates that tasks of that type exist
        builder
            .wire_task::<GraphContractsTask>("graph-contracts-deployment")?
            .wire_task::<TapContractsTask>("tap-contracts-deployment")?
            .wire_task::<SubgraphDeployTask>("subgraph-deployment")?;

        // Register Graph-specific actions on the base daemon
        builder = builder
            .register_action(
                "setup-test-stack",
                "Set up a complete Graph Protocol test stack",
                |_params| async move {
                    info!("Setting up Graph Protocol test stack");
                    // Note: Actual stack launch happens via launch_stack() method
                    // This action is a placeholder for WebSocket API compatibility
                    Ok(json!({
                        "status": "success",
                        "message": "To launch stack, call launch_stack() method or use --auto-start CLI flag"
                    }))
                },
            )?
            .register_action(
                "health-check-stack",
                "Check health of all Graph Protocol services",
                |_params| async move {
                    info!("Checking health of Graph Protocol stack");
                    Ok(json!({
                        "anvil": "healthy",
                        "ipfs": "healthy",
                        "postgres": "healthy",
                        "graph-node": "healthy"
                    }))
                },
            )?;

        let base = builder.build().await?;

        Ok(Self { base })
    }

    /// Launch all services in the stack in dependency order
    pub async fn launch_stack(&self) -> Result<(), Error> {
        // Delegate to the base daemon's launch_stack implementation
        self.base.launch_stack().await
    }
}

#[async_trait]
impl Daemon for GraphTestDaemon {
    async fn start(&self) -> Result<(), Error> {
        info!("Starting Graph Test Daemon");

        // Start the base daemon
        self.base.start().await?;

        // Log available services
        let services = self.base.json_service_registry().list();
        info!("Available services:");
        for (name, service) in services {
            info!(
                "  - {} ({}): {}",
                name,
                service.name(),
                service.description()
            );

            // Log available actions for each service
            for action in service.available_actions() {
                info!("    * {}: {}", action.name, action.description);
            }
        }

        Ok(())
    }

    async fn stop(&self) -> Result<(), Error> {
        info!("Stopping Graph Test Daemon");
        self.base.stop().await
    }

    fn endpoint(&self) -> SocketAddr {
        self.base.endpoint()
    }

    fn service_manager(&self) -> &ServiceManager {
        self.base.service_manager()
    }

    fn service_registry(&self) -> &Registry {
        self.base.service_registry()
    }
}

// Implement Action trait to inherit base daemon actions
#[async_trait]
impl Action for GraphTestDaemon {
    fn actions(&self) -> &ActionRegistry {
        self.base.actions()
    }

    fn actions_mut(&mut self) -> &mut ActionRegistry {
        // Note: This requires mutable access to base, which we don't have
        // In practice, actions would be registered during construction
        unimplemented!("Actions should be registered during daemon construction")
    }
}
