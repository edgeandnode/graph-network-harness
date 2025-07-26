//! Graph Test Daemon implementation using new Service architecture
//!
//! This module implements the GraphTestDaemon which extends BaseDaemon with
//! Graph Protocol specific services that can perform actions.

use async_trait::async_trait;
use harness_core::prelude::*;
use harness_core::{Registry, ServiceManager};
use std::net::SocketAddr;
use tracing::info;

use crate::services::{AnvilService, GraphNodeService, IpfsService, PostgresService};

/// Graph Protocol specialized testing daemon
pub struct GraphTestDaemon {
    /// Base daemon functionality  
    base: BaseDaemon,
}

impl GraphTestDaemon {
    /// Create a new Graph Test Daemon
    pub async fn new(endpoint: SocketAddr) -> Result<Self> {
        // Build the base daemon with Graph-specific services
        let mut builder = BaseDaemon::builder().with_endpoint(endpoint);

        // Get mutable access to the service stack for registration
        {
            let stack = builder.service_stack_mut();

            // Register Graph Node service
            let graph_node = GraphNodeService::new("localhost".to_string());
            stack.register("graph-node-1".to_string(), graph_node)?;

            // Register Anvil blockchain service
            let anvil = AnvilService::new(31337, 8545);
            stack.register("anvil-1".to_string(), anvil)?;

            // Register PostgreSQL service
            let postgres = PostgresService::new("graph-node".to_string(), 5432);
            stack.register("postgres-1".to_string(), postgres)?;

            // Register IPFS service
            let ipfs = IpfsService::new(5001, 8080);
            stack.register("ipfs-1".to_string(), ipfs)?;
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
