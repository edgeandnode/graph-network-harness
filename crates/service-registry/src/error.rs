//! Error types for the service registry

use thiserror::Error;

/// Service registry error type
#[derive(Error, Debug)]
pub enum Error {
    /// Service not found
    #[error("Service not found: {0}")]
    ServiceNotFound(String),
    
    /// Service already exists
    #[error("Service already exists: {0}")]
    ServiceExists(String),
    
    /// Invalid service state transition
    #[error("Invalid state transition: {from:?} -> {to:?}")]
    InvalidStateTransition {
        /// Current state
        from: crate::models::ServiceState,
        /// Attempted state
        to: crate::models::ServiceState,
    },
    
    /// WebSocket error
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tungstenite::Error),
    
    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    
    /// JSON serialization error
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    
    /// YAML serialization error
    #[error("YAML serialization error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    
    /// Package error
    #[error("Package error: {0}")]
    Package(String),
    
    /// Deployment error
    #[error("Deployment error: {0}")]
    Deployment(String),
    
    /// Command execution error
    #[error("Command execution error: {0}")]
    CommandExecution(#[from] command_executor::Error),
}

/// Result type alias
pub type Result<T> = std::result::Result<T, Error>;