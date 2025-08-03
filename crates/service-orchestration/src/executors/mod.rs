//! Service executor implementations.
//!
//! This module provides abstractions over command-executor for different
//! service execution environments.

pub mod attached;
pub mod docker;
pub mod process;
pub mod stream_utils;
pub mod traits;

pub use attached::{SystemdAttachedExecutor, DockerAttachedExecutor, LocalProcessAttachedExecutor};
pub use docker::DockerExecutor;
pub use process::ProcessExecutor;
pub use traits::{
    EventStreamable, ManagedService, AttachedService, EventStream as TraitEventStream
};

use crate::{Error, config::ServiceConfig, health::HealthStatus};
use async_trait::async_trait;
use command_executor::event::ProcessEvent;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Information about a running service instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunningService {
    /// Unique service instance ID
    pub id: Uuid,
    /// Service name
    pub name: String,
    /// Service configuration used to start this instance
    pub config: ServiceConfig,
    /// Process ID (if applicable)
    pub pid: Option<u32>,
    /// Container ID (if applicable)  
    pub container_id: Option<String>,
    /// Service endpoint information
    pub endpoints: HashMap<String, String>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
    /// Network information
    pub network_info: Option<NetworkInfo>,
}

/// Network information for a running service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    /// IP address
    pub ip: String,
    /// Primary port
    pub port: Option<u16>,
    /// All exposed ports
    pub ports: Vec<u16>,
    /// Hostname
    pub hostname: String,
}

impl RunningService {
    /// Create a new running service instance
    pub fn new(name: String, config: ServiceConfig) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            config,
            pid: None,
            container_id: None,
            endpoints: HashMap::new(),
            metadata: HashMap::new(),
            network_info: None,
        }
    }

    /// Set the process ID
    pub fn with_pid(mut self, pid: u32) -> Self {
        self.pid = Some(pid);
        self
    }

    /// Set the container ID
    pub fn with_container_id(mut self, container_id: String) -> Self {
        self.container_id = Some(container_id);
        self
    }

    /// Add an endpoint
    pub fn with_endpoint(mut self, name: String, endpoint: String) -> Self {
        self.endpoints.insert(name, endpoint);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Set network information
    pub fn with_network_info(mut self, network_info: NetworkInfo) -> Self {
        self.network_info = Some(network_info);
        self
    }
}

/// Event stream from a running service
pub type EventStream = futures::stream::BoxStream<'static, ProcessEvent>;

/// Service executor trait for managing service lifecycle
#[async_trait]
pub trait ServiceExecutor: Send + Sync {
    /// Start a service with the given configuration
    async fn start(&self, config: ServiceConfig) -> std::result::Result<RunningService, Error>;

    /// Stop a running service
    async fn stop(&self, service: &RunningService) -> std::result::Result<(), Error>;

    /// Check the health of a running service
    async fn health_check(&self, service: &RunningService) -> std::result::Result<HealthStatus, Error>;

    /// Stream events from a running service
    async fn stream_events(&self, service: &RunningService) -> std::result::Result<EventStream, Error>;

    /// Check if the executor can handle the given service configuration
    fn can_handle(&self, config: &ServiceConfig) -> bool;
}
