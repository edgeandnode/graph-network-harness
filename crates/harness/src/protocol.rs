//! Protocol types for daemon communication

use service_orchestration::{ServiceConfig, ServiceStatus};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

/// Request messages from client to daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Request {
    /// Start a service
    StartService {
        name: String,
        config: ServiceConfig,
    },
    
    /// Stop a service
    StopService {
        name: String,
    },
    
    /// Get status of a specific service
    GetServiceStatus {
        name: String,
    },
    
    /// List all services and their status
    ListServices,
    
    /// Run health checks
    RunHealthChecks,
    
    /// Shutdown the daemon
    Shutdown,
}

/// Response messages from daemon to client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Response {
    /// Operation succeeded
    Success,
    
    /// Operation failed
    Error {
        message: String,
    },
    
    /// Service status response
    ServiceStatus {
        status: ServiceStatus,
    },
    
    /// List of services and their status
    ServiceList {
        services: HashMap<String, ServiceStatus>,
    },
    
    /// Health check results
    HealthCheckResults {
        results: HashMap<String, String>,
    },
}