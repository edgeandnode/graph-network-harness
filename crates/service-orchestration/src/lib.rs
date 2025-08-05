//! # Orchestrator
//!
//! Heterogeneous service orchestration implementing ADR-007.
//!
//! This crate provides the core orchestration logic for managing services across
//! different execution environments (local processes, Docker containers, remote SSH)
//! while providing unified networking and service discovery.
//!
//! ## Example
//!
//! ```rust
//! use service_orchestration::{ServiceManager, ServiceConfig, ServiceTarget};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let mut manager = ServiceManager::new().await?;
//!
//! let config = ServiceConfig {
//!     name: "test-service".to_string(),
//!     target: ServiceTarget::Process {
//!         binary: "echo".to_string(),
//!         args: vec!["hello".to_string()],
//!         env: Default::default(),
//!         working_dir: None,
//!     },
//!     dependencies: vec![],
//!     health_check: None,
//! };
//!
//! manager.start_service("test-service", config).await?;
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]
#![warn(unsafe_code)]

mod config;
mod context;
mod discovery;
mod executors;
mod health;
mod health_integration;
mod manager;
mod orchestrator;
mod package;
mod state;
mod task_config;

pub use config::{
    Dependency, HealthCheck, RemoteMode, ServiceConfig, ServiceStatus, ServiceTarget,
};
pub use context::OrchestrationContext;
pub use discovery::{ConfigurationProvider, ServiceDiscovery, ServiceEndpoint};
pub use executors::{
    AttachedService, DockerAttachedExecutor, DockerExecutor, EventStream, EventStreamable,
    ManagedService, ProcessExecutor, RunningService, ServiceExecutor, SystemdAttachedExecutor,
};
pub use health::{HealthCheckable, HealthChecker, HealthMonitor, HealthStatus};
pub use health_integration::{HealthMonitoringExt, HealthMonitoringManager};
pub use manager::ServiceManager;
pub use orchestrator::{DependencyGraph, DependencyNode, DependencyOrchestrator};
pub use package::{
    DeployedPackage, PackageBuilder, PackageDeployer, PackageHealthCheck, PackageManifest,
    PackageService, RemoteTarget,
};
pub use state::{
    DeploymentState, DeploymentStatus, DeploymentSummary, ServiceDeploymentState, ServiceState,
    ServiceStateFilter, StateManager, TaskExecutionState, TaskState, TaskStateFilter,
};
pub use task_config::{ServiceInstanceConfig, StackConfig, TaskConfig};

/// Error types for orchestration operations
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// Service registry errors
    #[error("Service registry error: {0}")]
    Registry(#[from] service_registry::Error),

    /// Command executor errors  
    #[error("Command execution error: {0}")]
    CommandExecutor(#[from] command_executor::Error),

    /// Service not found
    #[error("Service not found: {0}")]
    ServiceNotFound(String),

    /// Service already exists
    #[error("Service already exists: {0}")]
    ServiceExists(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Network error
    #[error("Network error: {0}")]
    Network(String),

    /// Package deployment error
    #[error("Package deployment error: {0}")]
    Package(String),

    /// Health check error
    #[error("Health check error: {0}")]
    HealthCheck(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Not implemented error
    #[error("Not implemented: {0}")]
    NotImplemented(String),

    /// Other error
    #[error("Other error: {0}")]
    Other(String),
}
