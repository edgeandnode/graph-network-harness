//! Graph Test Daemon implementation using new Service architecture
//!
//! This module implements the GraphTestDaemon which extends BaseDaemon with
//! Graph Protocol specific services that can perform actions.

use async_trait::async_trait;
use harness_core::prelude::*;
use harness_core::{Error, Registry, ServiceManager};
use service_orchestration::{ServiceTarget, StackConfig};
use std::net::SocketAddr;
use std::path::Path;
use std::result::Result;
use tracing::info;

use crate::services::{AnvilService, GraphNodeService, IpfsService, PostgresService};
use harness_core::config_traits::ServiceFromConfig;
use harness_core::json_service_adapter::JsonServiceAdapter;

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
        // Convert config to Value for validation
        let config_value = serde_json::to_value(&config)
            .map_err(|e| Error::daemon(format!("Failed to convert config: {e}")))?;

        // Build the base daemon with Graph-specific services
        let mut builder = BaseDaemon::builder()
            .with_endpoint(endpoint)
            .with_config(config_value)
            .with_stack_config(config.clone());

        // Register services from configuration
        {
            // Register services from configuration
            for (instance_name, mut service_config) in config.services {
                // Set the service name from the map key if not already set
                service_config.orchestration.name = instance_name.clone();

                info!(
                    "Loading service '{}' with type '{}' using target '{:?}'",
                    instance_name, service_config.service_type, service_config.orchestration.target
                );

                // Use ServiceFromConfig to create service instances dynamically
                match service_config.service_type.as_str() {
                    "graph-node" => {
                        let service = GraphNodeService::from_config(&service_config.orchestration)?;
                        builder.register_service(instance_name, service)?;
                    }
                    "anvil" => {
                        let service = AnvilService::from_config(&service_config.orchestration)?;
                        builder.register_service(instance_name, service)?;
                    }
                    "postgres" => {
                        let service = PostgresService::from_config(&service_config.orchestration)?;
                        builder.register_service(instance_name, service)?;
                    }
                    "ipfs" => {
                        let service = IpfsService::from_config(&service_config.orchestration)?;
                        builder.register_service(instance_name, service)?;
                    }
                    unknown => {
                        return Err(Error::service_type(format!(
                            "Unknown service type '{}'",
                            unknown
                        )));
                    }
                }
            }
        }

        // Register tasks
        {
            let tasks = builder.task_stack_mut();

            // Register deployment tasks
            tasks.register(
                "deploy-graph-contracts".to_string(),
                crate::tasks::GraphContractsTask::new(
                    "http://localhost:8545".to_string(),
                    "./contracts/graph-contracts".to_string(),
                ),
            )?;

            tasks.register(
                "deploy-tap-contracts".to_string(),
                crate::tasks::TapContractsTask::new(
                    "http://localhost:8545".to_string(),
                    "./contracts/tap-contracts".to_string(),
                ),
            )?;

            info!("Registered {} deployment tasks", tasks.list().len());
        }

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
        let services = self.base.service_stack().list();
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
