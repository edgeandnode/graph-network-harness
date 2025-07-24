//! Protocol types for daemon communication

use serde::{Deserialize, Serialize};
use service_orchestration::{ServiceConfig, ServiceStatus};
use std::collections::HashMap;

/// Request messages from client to daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Request {
    /// Start a service
    StartService { name: String, config: ServiceConfig },

    /// Stop a service
    StopService { name: String },

    /// Get status of a specific service
    GetServiceStatus { name: String },

    /// List all services and their status
    ListServices,

    /// Run health checks
    RunHealthChecks,

    /// Shutdown the daemon
    Shutdown,
}

/// Service network information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceNetworkInfo {
    /// IP address of the service
    pub ip: String,
    /// Primary port exposed by the service (if any)
    pub port: Option<u16>,
    /// Hostname of the service
    pub hostname: String,
    /// All exposed ports
    pub ports: Vec<u16>,
}

/// Response messages from daemon to client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Response {
    /// Operation succeeded
    Success,

    /// Operation failed
    Error { message: String },

    /// Service started successfully with network info
    ServiceStarted { 
        name: String,
        network_info: ServiceNetworkInfo,
    },

    /// Service status response
    ServiceStatus { status: ServiceStatus },

    /// List of services and their status
    ServiceList {
        services: HashMap<String, ServiceStatus>,
    },

    /// Health check results
    HealthCheckResults { results: HashMap<String, String> },
}
