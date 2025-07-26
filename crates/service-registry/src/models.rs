//! Data models for the service registry

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;

/// A registered service entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEntry {
    /// Unique service identifier
    pub name: String,

    /// Service version
    pub version: String,

    /// How the service is executed
    pub execution: ExecutionInfo,

    /// Where the service runs
    pub location: Location,

    /// Network endpoints
    pub endpoints: Vec<Endpoint>,

    /// Service dependencies
    pub depends_on: Vec<String>,

    /// Current state
    pub state: ServiceState,

    /// Last health check result
    pub last_health_check: Option<HealthStatus>,

    /// When the service was registered
    pub registered_at: DateTime<Utc>,

    /// Last state change
    pub last_state_change: DateTime<Utc>,
}

/// How a service is executed
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExecutionInfo {
    /// Managed process we control
    ManagedProcess {
        /// Process ID if running
        pid: Option<u32>,
        /// Command that was executed
        command: String,
        /// Command arguments
        args: Vec<String>,
    },
    /// Docker container
    DockerContainer {
        /// Container ID
        container_id: Option<String>,
        /// Docker image
        image: String,
        /// Container name
        name: Option<String>,
    },
    /// Systemd service
    SystemdService {
        /// Systemd unit name
        unit_name: String,
    },
    /// Systemd portable service
    SystemdPortable {
        /// Portable image name
        image_name: String,
        /// Unit name
        unit_name: String,
    },
}

/// Where a service runs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Location {
    /// Running on the harness host
    Local,
    /// Running on a remote host
    Remote {
        /// Hostname or IP
        host: String,
        /// SSH username
        ssh_user: String,
        /// SSH port (default 22)
        ssh_port: Option<u16>,
    },
}

/// Network endpoint for a service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    /// Endpoint name (e.g., "http", "grpc", "metrics")
    pub name: String,

    /// Socket address
    pub address: SocketAddr,

    /// Protocol
    pub protocol: Protocol,

    /// Metadata (e.g., "path": "/api")
    pub metadata: HashMap<String, String>,
}

/// Network protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    /// HTTP protocol
    Http,
    /// HTTPS protocol
    Https,
    /// gRPC protocol
    Grpc,
    /// Raw TCP
    Tcp,
    /// WebSocket
    WebSocket,
    /// Custom protocol
    Custom(String),
}

/// Service state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    /// Service is registered but not started
    Registered,
    /// Service is starting up
    Starting,
    /// Service is running
    Running,
    /// Service is stopping
    Stopping,
    /// Service is stopped
    Stopped,
    /// Service has failed
    Failed,
}

/// Health check status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Is the service healthy?
    pub healthy: bool,

    /// Health check message
    pub message: Option<String>,

    /// When the check was performed
    pub checked_at: DateTime<Utc>,

    /// Check duration in milliseconds
    pub duration_ms: u64,
}

/// WebSocket message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsMessage {
    /// Client request
    Request {
        /// Request ID for correlation
        id: String,
        /// Action to perform
        action: Action,
        /// Action parameters
        params: serde_json::Value,
    },
    /// Server response
    Response {
        /// Request ID
        id: String,
        /// Response data
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<serde_json::Value>,
        /// Error information
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<ErrorInfo>,
    },
    /// Server-pushed event
    Event {
        /// Event type
        event: EventType,
        /// Event data
        data: serde_json::Value,
    },
}

/// Available actions
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// List all services
    ListServices,
    /// Get specific service
    GetService,
    /// Perform service action (start/stop/restart)
    ServiceAction,
    /// List all endpoints
    ListEndpoints,
    /// Deploy a package
    DeployPackage,
    /// Subscribe to events
    Subscribe,
    /// Unsubscribe from events
    Unsubscribe,
}

/// Service actions
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceAction {
    /// Start the service
    Start,
    /// Stop the service
    Stop,
    /// Restart the service
    Restart,
    /// Reload configuration
    Reload,
}

/// Event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// Service was registered
    ServiceRegistered,
    /// Service was updated
    ServiceUpdated,
    /// Service was deregistered
    ServiceDeregistered,
    /// Service state changed
    ServiceStateChanged,
    /// Endpoint was updated
    EndpointUpdated,
    /// Deployment progress
    DeploymentProgress,
    /// Health check result
    HealthCheckResult,
    /// Registry loaded from disk
    RegistryLoaded,
}

/// Error information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorInfo {
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
    /// Additional details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ServiceEntry {
    /// Create a new service entry
    pub fn new(
        name: String,
        version: String,
        execution: ExecutionInfo,
        location: Location,
    ) -> crate::Result<Self> {
        // Validate service name
        if name.trim().is_empty() {
            return Err(crate::Error::Package(
                "Service name cannot be empty".to_string(),
            ));
        }

        // Validate version
        if version.trim().is_empty() {
            return Err(crate::Error::Package(
                "Service version cannot be empty".to_string(),
            ));
        }

        let now = Utc::now();
        Ok(Self {
            name,
            version,
            execution,
            location,
            endpoints: Vec::new(),
            depends_on: Vec::new(),
            state: ServiceState::Registered,
            last_health_check: None,
            registered_at: now,
            last_state_change: now,
        })
    }

    /// Update service state
    pub fn update_state(&mut self, new_state: ServiceState) {
        self.state = new_state;
        self.last_state_change = Utc::now();
    }

    /// Add an endpoint
    pub fn add_endpoint(&mut self, endpoint: Endpoint) {
        // Remove existing endpoint with same name
        self.endpoints.retain(|e| e.name != endpoint.name);
        self.endpoints.push(endpoint);
    }

    /// Add a dependency
    pub fn add_dependency(&mut self, service_name: String) {
        if !self.depends_on.contains(&service_name) {
            self.depends_on.push(service_name);
        }
    }

    /// Check if service has a specific endpoint
    pub fn has_endpoint(&self, name: &str) -> bool {
        self.endpoints.iter().any(|e| e.name == name)
    }

    /// Get endpoint by name
    pub fn get_endpoint(&self, name: &str) -> Option<&Endpoint> {
        self.endpoints.iter().find(|e| e.name == name)
    }
}

impl Endpoint {
    /// Create a new endpoint
    pub fn new(name: String, address: SocketAddr, protocol: Protocol) -> Self {
        Self {
            name,
            address,
            protocol,
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to the endpoint
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Get metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(|s| s.as_str())
    }
}
