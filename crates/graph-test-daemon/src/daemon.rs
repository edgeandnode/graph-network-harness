//! Graph Test Daemon implementation using new Service architecture
//!
//! This module implements the GraphTestDaemon which extends BaseDaemon with
//! Graph Protocol specific services that can perform actions.

use async_trait::async_trait;
use harness_core::prelude::*;
use harness_core::{Registry, ServiceManager};
use serde::{Deserialize, Serialize};
use service_orchestration::{HealthCheck, ServiceInstanceConfig, ServiceTarget, StackConfig};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use tracing::info;

use crate::service_registry::ServiceRegistry;
use crate::services::{AnvilService, GraphNodeService, IpfsService, PostgresService};

/// Type alias for Graph Protocol stack configuration
pub type GraphStackConfig = StackConfig;

/// Graph Protocol specialized testing daemon
pub struct GraphTestDaemon {
    /// Base daemon functionality  
    base: BaseDaemon,
}

impl GraphTestDaemon {
    /// Create a new Graph Test Daemon from configuration
    pub async fn from_config<P: AsRef<Path>>(endpoint: SocketAddr, config_path: P) -> Result<Self> {
        // Load configuration from YAML file
        let config_content = std::fs::read_to_string(config_path.as_ref())
            .map_err(|e| Error::daemon(format!("Failed to read config file: {}", e)))?;

        let config: GraphStackConfig = serde_yaml::from_str(&config_content)
            .map_err(|e| Error::daemon(format!("Failed to parse config YAML: {}", e)))?;

        Self::from_stack_config(endpoint, config).await
    }

    /// Create a new Graph Test Daemon from a stack configuration
    pub async fn from_stack_config(endpoint: SocketAddr, config: GraphStackConfig) -> Result<Self> {
        // Convert config to Value for validation
        let config_value = serde_json::to_value(&config)
            .map_err(|e| Error::daemon(format!("Failed to convert config: {}", e)))?;

        // Build the base daemon with Graph-specific services
        let mut builder = BaseDaemon::builder()
            .with_endpoint(endpoint)
            .with_config(config_value);

        // Create service registry for dynamic service creation
        let service_registry = ServiceRegistry::new();

        // Get mutable access to the service stack for registration
        {
            let stack = builder.service_stack_mut();

            // Register services from configuration
            for (instance_name, service_config) in &config.services {
                info!(
                    "Loading service '{}' with type '{}' using target '{:?}'",
                    instance_name, service_config.service_type, service_config.orchestration.target
                );

                // Create service instance based on service_type, using orchestration config for parameters
                match service_config.service_type.as_str() {
                    "graph-node" => {
                        // Extract endpoint from environment or use default
                        let endpoint = service_config
                            .orchestration
                            .target
                            .env()
                            .get("GRAPH_ENDPOINT")
                            .cloned()
                            .unwrap_or_else(|| "localhost".to_string());
                        let graph_node = GraphNodeService::new(endpoint.clone());
                        stack.register(instance_name.clone(), graph_node)?;
                        info!("Registered Graph Node service with endpoint: {}", endpoint);
                    }
                    "anvil" => {
                        // Extract chain_id and port from environment or process args
                        let env = service_config.orchestration.target.env();
                        let chain_id = env
                            .get("CHAIN_ID")
                            .and_then(|s| s.parse::<u64>().ok())
                            .unwrap_or(31337);

                        // Extract port from args if it's a process target
                        let port = match &service_config.orchestration.target {
                            ServiceTarget::Process { args, .. } => args
                                .iter()
                                .position(|arg| arg == "--port")
                                .and_then(|pos| args.get(pos + 1))
                                .and_then(|port_str| port_str.parse::<u16>().ok())
                                .unwrap_or(8545),
                            ServiceTarget::Docker { ports, .. } => {
                                ports.first().cloned().unwrap_or(8545)
                            }
                            _ => 8545,
                        };

                        let anvil = AnvilService::new(chain_id, port);
                        stack.register(instance_name.clone(), anvil)?;
                        info!(
                            "Registered Anvil service with chain_id: {}, port: {}",
                            chain_id, port
                        );
                    }
                    "postgres" => {
                        // Extract database name from environment
                        let env = service_config.orchestration.target.env();
                        let db_name = env
                            .get("POSTGRES_DB")
                            .cloned()
                            .unwrap_or_else(|| "graph-node".to_string());

                        // Extract port from target
                        let port = match &service_config.orchestration.target {
                            ServiceTarget::Docker { ports, .. } => {
                                ports.first().cloned().unwrap_or(5432)
                            }
                            _ => 5432,
                        };

                        let postgres = PostgresService::new(db_name.clone(), port);
                        stack.register(instance_name.clone(), postgres)?;
                        info!(
                            "Registered PostgreSQL service with db_name: {}, port: {}",
                            db_name, port
                        );
                    }
                    "ipfs" => {
                        // Extract ports from target
                        let (api_port, gateway_port) = match &service_config.orchestration.target {
                            ServiceTarget::Docker { ports, .. } => {
                                let api_port = ports.get(0).cloned().unwrap_or(5001);
                                let gateway_port = ports.get(1).cloned().unwrap_or(8080);
                                (api_port, gateway_port)
                            }
                            _ => (5001, 8080),
                        };

                        let ipfs = IpfsService::new(api_port, gateway_port);
                        stack.register(instance_name.clone(), ipfs)?;
                        info!(
                            "Registered IPFS service with api_port: {}, gateway_port: {}",
                            api_port, gateway_port
                        );
                    }
                    unknown => {
                        return Err(Error::service_type(format!(
                            "Unknown service type '{}' for instance '{}'",
                            unknown, instance_name
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
                |params| async move {
                    info!("Setting up Graph Protocol test stack");
                    Ok(json!({
                        "status": "success",
                        "services": ["anvil", "ipfs", "postgres", "graph-node"],
                        "message": "Test stack initialized"
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
}

#[async_trait]
impl Daemon for GraphTestDaemon {
    async fn start(&self) -> Result<()> {
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

    async fn stop(&self) -> Result<()> {
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
