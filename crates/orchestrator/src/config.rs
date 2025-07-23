//! Service configuration types and utilities.
//!
//! This module defines the configuration model for services that matches
//! the ADR-007 specification for heterogeneous service orchestration.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for a service to be managed by the orchestrator
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServiceConfig {
    /// Unique service name
    pub name: String,
    /// Where and how to run the service
    pub target: ServiceTarget,
    /// Services this service depends on
    pub dependencies: Vec<String>,
    /// Optional health check configuration
    pub health_check: Option<HealthCheck>,
}

/// Service execution target specification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ServiceTarget {
    /// Local process execution
    Process {
        /// Binary to execute
        binary: String,
        /// Command line arguments
        args: Vec<String>,
        /// Environment variables
        env: HashMap<String, String>,
        /// Working directory (optional)
        working_dir: Option<String>,
    },
    /// Docker container execution
    Docker {
        /// Container image
        image: String,
        /// Environment variables
        env: HashMap<String, String>,
        /// Port mappings (host ports)
        ports: Vec<u16>,
        /// Volume mounts
        volumes: Vec<String>,
    },
    /// Remote LAN execution via SSH
    RemoteLan {
        /// Remote host address
        host: String,
        /// SSH username
        user: String,
        /// Binary to execute on remote host
        binary: String,
        /// Command line arguments
        args: Vec<String>,
    },
    /// WireGuard network execution with package deployment
    Wireguard {
        /// WireGuard peer address
        host: String,
        /// SSH username
        user: String,
        /// Path to package tarball for deployment
        package: String,
    },
}

impl ServiceTarget {
    /// Get environment variables from the target
    pub fn env(&self) -> HashMap<String, String> {
        match self {
            ServiceTarget::Process { env, .. } => env.clone(),
            ServiceTarget::Docker { env, .. } => env.clone(),
            ServiceTarget::RemoteLan { .. } => HashMap::new(),
            ServiceTarget::Wireguard { .. } => HashMap::new(),
        }
    }

    /// Create a new target with updated environment variables
    pub fn with_env(&self, new_env: HashMap<String, String>) -> Self {
        match self {
            ServiceTarget::Process { binary, args, working_dir, .. } => ServiceTarget::Process {
                binary: binary.clone(),
                args: args.clone(),
                env: new_env,
                working_dir: working_dir.clone(),
            },
            ServiceTarget::Docker { image, ports, volumes, .. } => ServiceTarget::Docker {
                image: image.clone(),
                env: new_env,
                ports: ports.clone(),
                volumes: volumes.clone(),
            },
            ServiceTarget::RemoteLan { host, user, binary, args } => ServiceTarget::RemoteLan {
                host: host.clone(),
                user: user.clone(),
                binary: binary.clone(),
                args: args.clone(),
            },
            ServiceTarget::Wireguard { host, user, package } => ServiceTarget::Wireguard {
                host: host.clone(),
                user: user.clone(),
                package: package.clone(),
            },
        }
    }
}

impl ServiceConfig {
    /// Create a new config with updated environment variables
    pub fn with_env(&self, env: HashMap<String, String>) -> Self {
        ServiceConfig {
            name: self.name.clone(),
            target: self.target.with_env(env),
            dependencies: self.dependencies.clone(),
            health_check: self.health_check.clone(),
        }
    }
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthCheck {
    /// Command to run for health check
    pub command: String,
    /// Arguments for health check command
    pub args: Vec<String>,
    /// Interval between health checks in seconds
    pub interval: u64,
    /// Number of consecutive failures before marking unhealthy
    pub retries: u32,
    /// Timeout for each health check in seconds
    pub timeout: u64,
}

/// Current status of a service
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ServiceStatus {
    /// Service is not running
    Stopped,
    /// Service is starting up
    Starting,
    /// Service is running and healthy
    Running,
    /// Service is running but unhealthy
    Unhealthy,
    /// Service has failed
    Failed(String),
}

impl Default for ServiceStatus {
    fn default() -> Self {
        ServiceStatus::Stopped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_config_serialization() {
        let config = ServiceConfig {
            name: "test-service".to_string(),
            target: ServiceTarget::Process {
                binary: "echo".to_string(),
                args: vec!["hello".to_string()],
                env: HashMap::from([("FOO".to_string(), "bar".to_string())]),
                working_dir: Some("/tmp".to_string()),
            },
            dependencies: vec!["database".to_string()],
            health_check: Some(HealthCheck {
                command: "curl".to_string(),
                args: vec!["http://localhost:8080/health".to_string()],
                interval: 30,
                retries: 3,
                timeout: 10,
            }),
        };

        let yaml = serde_yaml::to_string(&config).expect("Failed to serialize");
        let deserialized: ServiceConfig = serde_yaml::from_str(&yaml).expect("Failed to deserialize");
        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_service_target_with_env() {
        let mut env = HashMap::new();
        env.insert("KEY1".to_string(), "value1".to_string());

        let target = ServiceTarget::Process {
            binary: "test".to_string(),
            args: vec![],
            env: HashMap::new(),
            working_dir: None,
        };

        let updated = target.with_env(env.clone());
        assert_eq!(updated.env(), env);
    }

    #[test]
    fn test_docker_target_serialization() {
        let target = ServiceTarget::Docker {
            image: "nginx:latest".to_string(),
            env: HashMap::from([("ENV_VAR".to_string(), "value".to_string())]),
            ports: vec![80, 443],
            volumes: vec!["/data:/app/data".to_string()],
        };

        let yaml = serde_yaml::to_string(&target).expect("Failed to serialize");
        let deserialized: ServiceTarget = serde_yaml::from_str(&yaml).expect("Failed to deserialize");
        assert_eq!(target, deserialized);
    }
}