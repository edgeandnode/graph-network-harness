//! Graph Test Daemon implementation using new Service architecture
//!
//! This module implements the GraphTestDaemon which extends BaseDaemon with
//! Graph Protocol specific services that can perform actions.

use async_trait::async_trait;
use harness_core::prelude::*;
use harness_core::{Error, Registry, ServiceManager};
use service_orchestration::StackConfig;
use std::net::SocketAddr;
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

