//! # Harness Configuration
//!
//! YAML configuration parser for the graph-network-harness.
//!
//! This crate provides the ability to parse services.yaml files and convert them
//! into the orchestrator's configuration types.

#![warn(missing_docs)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

pub mod parser;
pub mod resolver;

/// Configuration error types
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Failed to read configuration file
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),

    /// Failed to parse YAML
    #[error("Failed to parse YAML: {0}")]
    YamlError(#[from] serde_yaml::Error),

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    ValidationError(String),

    /// Environment variable not found
    #[error("Environment variable not found: {0}")]
    EnvVarNotFound(String),

    /// Service reference not found
    #[error("Service '{0}' not found")]
    ServiceNotFound(String),
}

/// Result type for configuration operations
pub type Result<T> = std::result::Result<T, ConfigError>;

/// Root configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Configuration version
    pub version: String,

    /// Optional deployment name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Global settings
    #[serde(default, skip_serializing_if = "Settings::is_default")]
    pub settings: Settings,

    /// Network definitions
    #[serde(default)]
    pub networks: HashMap<String, Network>,

    /// Service definitions
    pub services: HashMap<String, Service>,
}

/// Global settings
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Settings {
    /// Default log level
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_level: Option<String>,

    /// Default health check interval in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_check_interval: Option<u64>,

    /// Default startup timeout in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup_timeout: Option<u64>,

    /// Default shutdown timeout in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shutdown_timeout: Option<u64>,
}

impl Settings {
    /// Check if settings are default (all None)
    fn is_default(&self) -> bool {
        self == &Settings::default()
    }
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Network {
    /// Local network (same machine)
    #[serde(rename = "local")]
    Local {
        /// Optional subnet for IP allocation
        #[serde(skip_serializing_if = "Option::is_none")]
        subnet: Option<String>,
    },

    /// LAN network
    #[serde(rename = "lan")]
    Lan {
        /// Subnet for IP allocation
        subnet: String,
        /// Pre-defined nodes
        #[serde(default)]
        nodes: Vec<LanNode>,
    },

    /// WireGuard network
    #[serde(rename = "wireguard")]
    WireGuard {
        /// Subnet for IP allocation
        subnet: String,
        /// WireGuard config path
        #[serde(skip_serializing_if = "Option::is_none")]
        config_path: Option<String>,
        /// Pre-defined nodes
        #[serde(default)]
        nodes: Vec<WireGuardNode>,
    },
}

/// LAN node definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanNode {
    /// Host address
    pub host: String,
    /// Optional node name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// SSH username
    pub ssh_user: String,
    /// SSH key path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_key: Option<String>,
}

/// WireGuard node definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireGuardNode {
    /// Host address
    pub host: String,
    /// Optional node name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// SSH username
    pub ssh_user: String,
    /// SSH key path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_key: Option<String>,
    /// Enable package deployment
    #[serde(default)]
    pub package_deploy: bool,
}

/// Service definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    /// Service type
    #[serde(flatten)]
    pub service_type: ServiceType,

    /// Network to attach to
    pub network: String,

    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Service dependencies
    #[serde(default)]
    pub dependencies: Vec<String>,

    /// Optional health check
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_check: Option<HealthCheck>,

    /// Startup timeout in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup_timeout: Option<u64>,

    /// Shutdown timeout in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shutdown_timeout: Option<u64>,
}

/// Service type variants
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServiceType {
    /// Docker container service
    #[serde(rename = "docker")]
    Docker {
        /// Container image
        image: String,
        /// Port mappings
        #[serde(default)]
        ports: Vec<PortMapping>,
        /// Volume mounts
        #[serde(default)]
        volumes: Vec<String>,
        /// Container command override
        #[serde(skip_serializing_if = "Option::is_none")]
        command: Option<Vec<String>>,
        /// Container entrypoint override
        #[serde(skip_serializing_if = "Option::is_none")]
        entrypoint: Option<Vec<String>>,
    },

    /// Local process service
    #[serde(rename = "process")]
    Process {
        /// Binary to execute
        binary: String,
        /// Command line arguments
        #[serde(default)]
        args: Vec<String>,
        /// Working directory
        #[serde(skip_serializing_if = "Option::is_none")]
        working_dir: Option<String>,
        /// Run as user
        #[serde(skip_serializing_if = "Option::is_none")]
        user: Option<String>,
    },

    /// Remote service (via SSH)
    #[serde(rename = "remote")]
    Remote {
        /// Remote host (can be node name or IP)
        host: String,
        /// Binary to execute
        binary: String,
        /// Command line arguments
        #[serde(default)]
        args: Vec<String>,
        /// Working directory on remote
        #[serde(skip_serializing_if = "Option::is_none")]
        working_dir: Option<String>,
    },

    /// Package deployment service
    #[serde(rename = "package")]
    Package {
        /// Target host (can be node name or IP)
        host: String,
        /// Package file path
        package: String,
        /// Package version
        #[serde(skip_serializing_if = "Option::is_none")]
        version: Option<String>,
        /// Install path override
        #[serde(skip_serializing_if = "Option::is_none")]
        install_path: Option<String>,
    },
}

/// Port mapping for Docker containers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PortMapping {
    /// Simple port number (container port only)
    Simple(u16),
    /// Full mapping "host:container"
    Full(String),
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    /// Health check method
    #[serde(flatten)]
    pub check_type: HealthCheckType,

    /// Check interval in seconds
    #[serde(default = "default_interval")]
    pub interval: u64,

    /// Number of retries before marking unhealthy
    #[serde(default = "default_retries")]
    pub retries: u32,

    /// Timeout per check in seconds
    #[serde(default = "default_timeout")]
    pub timeout: u64,

    /// Grace period before first check
    #[serde(default)]
    pub start_period: u64,
}

/// Health check type variants
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HealthCheckType {
    /// Command-based health check
    Command {
        /// Command to run
        command: String,
        /// Command arguments
        #[serde(default)]
        args: Vec<String>,
    },

    /// HTTP health check
    Http {
        /// HTTP endpoint URL
        http: String,
    },

    /// TCP port check
    Tcp {
        /// TCP port to check
        tcp: TcpCheck,
    },
}

/// TCP health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpCheck {
    /// Port number
    pub port: u16,
    /// Connection timeout
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

// Default values for health checks
fn default_interval() -> u64 {
    30
}
fn default_retries() -> u32 {
    3
}
fn default_timeout() -> u64 {
    10
}
