//! Error types for harness-core

use thiserror::Error;

/// Result type alias for harness-core operations
pub type Result<T> = std::result::Result<T, Error>;

/// Core harness error types
#[derive(Error, Debug)]
pub enum Error {
    /// Service orchestration error
    #[error("Service orchestration error: {0}")]
    ServiceOrchestration(#[from] service_orchestration::Error),

    /// Service registry error
    #[error("Service registry error: {0}")]
    ServiceRegistry(#[from] service_registry::Error),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(#[from] harness_config::ConfigError),

    /// Action error
    #[error("Action error: {message}")]
    Action {
        /// Error message
        message: String,
    },

    /// Service type error
    #[error("Service type error: {message}")]
    ServiceType {
        /// Error message
        message: String,
    },

    /// Client communication error
    #[error("Client error: {0}")]
    Client(String),

    /// Daemon lifecycle error
    #[error("Daemon error: {0}")]
    Daemon(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// WebSocket error
    #[error("WebSocket error: {0}")]
    WebSocket(String),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl Error {
    /// Create an action error
    pub fn action(message: impl Into<String>) -> Self {
        Self::Action {
            message: message.into(),
        }
    }

    /// Create a service type error
    pub fn service_type(message: impl Into<String>) -> Self {
        Self::ServiceType {
            message: message.into(),
        }
    }

    /// Create a client error
    pub fn client(message: impl Into<String>) -> Self {
        Self::Client(message.into())
    }

    /// Create a daemon error
    pub fn daemon(message: impl Into<String>) -> Self {
        Self::Daemon(message.into())
    }

    /// Create a WebSocket error
    pub fn websocket(message: impl Into<String>) -> Self {
        Self::WebSocket(message.into())
    }
}
