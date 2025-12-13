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
//! ```rust,ignore
//! use service_orchestration::{ServiceManager, ServiceConfig, ServiceTarget, ProcessCommand};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let mut manager = ServiceManager::new().await?;
//!
//! let config = ServiceConfig {
//!     name: "test-service".to_string(),
//!     target: ServiceTarget::Process {
//!         command: ProcessCommand::Legacy { command: "echo hello".to_string() },
//!         env: Default::default(),
//!         ports: Default::default(),
//!         resources: None,
//!         working_dir: None,
//!         complete_if: None,
//!     },
//!     depends_on: vec![],
//!     health_check: None,
//!     templates: vec![],
//!     allocated_ports: Default::default(),
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
mod dependency_graph;
// mod discovery; // TODO: Refactor to remove service-registry dependency
mod executors;
mod health;
// mod health_integration; // TODO: Refactor to remove service-registry dependency
mod manager;
mod ports;
mod resources;
mod state;
mod template;
mod task_config;
mod task_executors;
mod task_manager;

pub use config::{
    CommandSpec, Dependency, HealthCheck, ParamValue, ProcessCommand, RemoteMode, ServiceConfig,
    ServiceStatus, ServiceTarget, TemplateConfig,
};
pub use ports::{PortAllocator, PortConfig, PortError, PortRegistry, PortSpec};
pub use resources::{ByteSize, CpuLimit, ResourceLimits};
pub use context::OrchestrationContext;
pub use dependency_graph::{DependencyGraph, DependencyNode};
// pub use discovery::{ConfigurationProvider, ServiceDiscovery, ServiceEndpoint}; // TODO: Refactor
pub use executors::{
    AttachedExecutor, AttachedService, DockerExecutor, EventStreamable, ManagedService,
    ProcessExecutor, RunningService, ServiceExecutor,
};
pub use health::{HealthCheckable, HealthChecker, HealthMonitor, HealthStatus};
// pub use health_integration::{HealthMonitoringExt, HealthMonitoringManager}; // TODO: Refactor
pub use manager::ServiceManager;
pub use state::{
    DeploymentState, DeploymentStatus, DeploymentSummary, ServiceDeploymentState, ServiceState,
    ServiceStateFilter, StateManager, TaskExecutionState, TaskState, TaskStateFilter,
};
pub use task_config::{ServiceInstanceConfig, StackConfig, TaskConfig};
pub use task_executors::ProcessTaskExecutor;
pub use task_manager::{TaskExecution, TaskExecutor, TaskManager, TaskStatus, TypedTaskProvider};
pub use template::{RunContext, TemplateError, TemplateProcessor};

// Re-export with the old name for backwards compatibility during transition
#[deprecated(note = "Use OrchestrationError instead")]
pub use OrchestrationError as Error;

/// Error types for orchestration operations
#[derive(thiserror::Error, Debug)]
pub enum OrchestrationError {
    // Registry errors removed - no longer using service-registry
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

    /// Port allocation error
    #[error("Port allocation error: {0}")]
    Port(#[from] crate::ports::PortError),

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
