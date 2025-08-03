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
mod executors;
mod health;
mod manager;
mod package;

pub use config::{HealthCheck, ServiceConfig, ServiceStatus, ServiceTarget};
pub use executors::{
    AttachedService, DockerAttachedExecutor, DockerExecutor, EventStream, EventStreamable,
    ManagedService, ProcessExecutor, RunningService, ServiceExecutor, SystemdAttachedExecutor,
};
pub use health::{HealthCheckable, HealthChecker, HealthMonitor, HealthStatus};
pub use manager::ServiceManager;
pub use package::{
    DeployedPackage, PackageBuilder, PackageDeployer, PackageHealthCheck, PackageManifest,
    PackageService, RemoteTarget,
};

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
