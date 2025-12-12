//! Graph Test Daemon implementation using new Service architecture
//!
//! This module implements the GraphTestDaemon which extends BaseDaemon with
//! Graph Protocol specific services that can perform actions.

use async_trait::async_trait;
use harness_core::daemon::{AutoWire, DaemonBuilder};
use harness_core::prelude::*;
use harness_core::{Error, ServiceManager};
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
    pub base: BaseDaemon,
}

impl AutoWire for GraphTestDaemon {
    fn auto_wire_types(builder: &mut DaemonBuilder) -> Result<(), Error> {
        // Wire all known service types
        builder
            .wire_service_type::<GraphNodeService>()?
            .wire_service_type::<AnvilService>()?
            .wire_service_type::<PostgresService>()?
            .wire_service_type::<IpfsService>()?;

        // Wire all known task types
        builder
            .wire_task_type::<GraphContractsTask>()?
            .wire_task_type::<TapContractsTask>()?
            .wire_task_type::<SubgraphDeployTask>()?;

        Ok(())
    }
}

impl GraphTestDaemon {
    /// Create a new Graph Test Daemon from a pre-configured builder
    /// This allows the caller to control exactly which services and tasks are registered
    pub async fn from_builder(
        mut builder: harness_core::daemon::DaemonBuilder,
    ) -> Result<Self, Error> {
        // Auto-wire all known types before building
        Self::auto_wire_types(&mut builder)?;
        let base = builder.build().await?;
        Ok(Self { base })
    }

    /// Create a new Graph Test Daemon from a stack configuration
    /// This registers all known Graph Protocol services and tasks that exist in the config
    pub async fn from_stack_config(
        endpoint: SocketAddr,
        config: GraphStackConfig,
    ) -> Result<Self, Error> {
        // Build the base daemon with Graph-specific services and auto-wire
        let mut builder = BaseDaemon::builder(config).with_endpoint(endpoint);

        builder.with_auto_wire::<Self>()?;

        let base = builder.build().await?;

        Ok(Self { base })
    }

    /// Launch all services in the stack in dependency order
    pub async fn launch_stack(&self) -> Result<Receiver<DaemonEvent>, Error> {
        // Delegate to the base daemon's launch_stack implementation
        self.base.launch_stack().await
    }
}

#[async_trait]
impl Daemon for GraphTestDaemon {

    // TODO: customize for graph-test-daemon
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
}
