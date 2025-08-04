//! Service configuration types and utilities.
//!
//! This module defines the configuration model for services that matches
//! the ADR-007 specification for heterogeneous service orchestration.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Dependency specification for services and tasks
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Dependency {
    /// Dependency on a service
    Service { service: String },
    /// Dependency on a task
    Task { task: String },
}

/// Configuration for a service to be managed by the orchestrator
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServiceConfig {
    /// Unique service name
    pub name: String,
    /// Where and how to run the service
    pub target: ServiceTarget,
    /// Services and tasks this service depends on
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    /// Optional health check configuration
    pub health_check: Option<HealthCheck>,
}

/// Service execution target specification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ServiceTarget {
    /// Local process execution (managed)
    #[serde(rename = "process")]
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
    /// Docker container execution (managed)
    #[serde(rename = "docker")]
    Docker {
        /// Container image
        image: String,
        /// Environment variables
        env: HashMap<String, String>,
        /// Port mappings (host ports)
        ports: Vec<u16>,
        /// Volume mounts
        #[serde(default)]
        volumes: Vec<String>,
    },
    /// Attach to existing Docker container
    #[serde(rename = "docker-attach")]
    DockerAttach {
        /// Container name or ID to attach to
        container: String,
        /// Environment variables
        env: HashMap<String, String>,
    },
    /// Attach to existing local process
    #[serde(rename = "process-attach")]
    ProcessAttach {
        /// Process ID to attach to
        pid: Option<u32>,
        /// Process name to search for
        process_name: Option<String>,
        /// Environment variables
        env: HashMap<String, String>,
    },
    /// Remote execution via SSH (replaces RemoteLan/Wireguard)
    #[serde(rename = "remote")]
    Remote {
        /// Remote host address
        host: String,
        /// SSH username
        user: String,
        /// Execution mode
        #[serde(flatten)]
        mode: RemoteMode,
        /// Environment variables
        env: HashMap<String, String>,
    },
    /// Remote LAN execution via SSH (deprecated, use Remote)
    #[deprecated(note = "Use Remote variant instead")]
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
    /// WireGuard network execution with package deployment (deprecated, use Remote)
    #[deprecated(note = "Use Remote variant instead")]
    Wireguard {
        /// WireGuard peer address
        host: String,
        /// SSH username
        user: String,
        /// Path to package tarball for deployment
        package: String,
    },
}

/// Remote execution mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum RemoteMode {
    /// Execute a binary on the remote host
    Process {
        /// Binary to execute
        binary: String,
        /// Command line arguments
        args: Vec<String>,
    },
    /// Deploy a package to the remote host
    Package {
        /// Path to package tarball
        package: String,
    },
}

impl ServiceTarget {
    /// Get environment variables from the target
    pub fn env(&self) -> HashMap<String, String> {
        match self {
            ServiceTarget::Process { env, .. } => env.clone(),
            ServiceTarget::Docker { env, .. } => env.clone(),
            ServiceTarget::DockerAttach { env, .. } => env.clone(),
            ServiceTarget::ProcessAttach { env, .. } => env.clone(),
            ServiceTarget::Remote { env, .. } => env.clone(),
            #[allow(deprecated)]
            ServiceTarget::RemoteLan { .. } => HashMap::new(),
            #[allow(deprecated)]
            ServiceTarget::Wireguard { .. } => HashMap::new(),
        }
    }

    /// Create a new target with updated environment variables
    pub fn with_env(&self, new_env: HashMap<String, String>) -> Self {
        match self {
            ServiceTarget::Process {
                binary,
                args,
                working_dir,
                ..
            } => ServiceTarget::Process {
                binary: binary.clone(),
                args: args.clone(),
                env: new_env,
                working_dir: working_dir.clone(),
            },
            ServiceTarget::Docker {
                image,
                ports,
                volumes,
                ..
            } => ServiceTarget::Docker {
                image: image.clone(),
                env: new_env,
                ports: ports.clone(),
                volumes: volumes.clone(),
            },
            ServiceTarget::DockerAttach { container, .. } => ServiceTarget::DockerAttach {
                container: container.clone(),
                env: new_env,
            },
            ServiceTarget::ProcessAttach {
                pid, process_name, ..
            } => ServiceTarget::ProcessAttach {
                pid: *pid,
                process_name: process_name.clone(),
                env: new_env,
            },
            ServiceTarget::Remote {
                host, user, mode, ..
            } => ServiceTarget::Remote {
                host: host.clone(),
                user: user.clone(),
                mode: mode.clone(),
                env: new_env,
            },
            #[allow(deprecated)]
            ServiceTarget::RemoteLan {
                host,
                user,
                binary,
                args,
            } => ServiceTarget::RemoteLan {
                host: host.clone(),
                user: user.clone(),
                binary: binary.clone(),
                args: args.clone(),
            },
            #[allow(deprecated)]
            ServiceTarget::Wireguard {
                host,
                user,
                package,
            } => ServiceTarget::Wireguard {
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

impl Default for HealthCheck {
    fn default() -> Self {
        Self {
            command: "true".to_string(),
            args: vec![],
            interval: 30,
            retries: 3,
            timeout: 10,
        }
    }
}

/// Current status of a service
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum ServiceStatus {
    /// Service is not running
    #[default]
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
            dependencies: vec![
                Dependency::Service {
                    service: "database".to_string(),
                },
            ],
            health_check: Some(HealthCheck {
                command: "curl".to_string(),
                args: vec!["http://localhost:8080/health".to_string()],
                interval: 30,
                retries: 3,
                timeout: 10,
            }),
        };

        let yaml = serde_yaml::to_string(&config).expect("Failed to serialize");
        let deserialized: ServiceConfig =
            serde_yaml::from_str(&yaml).expect("Failed to deserialize");
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
        let deserialized: ServiceTarget =
            serde_yaml::from_str(&yaml).expect("Failed to deserialize");
        assert_eq!(target, deserialized);
    }

    #[test]
    fn test_dependency_parsing() {
        // Test service dependency
        let service_dep = Dependency::Service {
            service: "postgres".to_string(),
        };
        let yaml = serde_yaml::to_string(&service_dep).expect("Failed to serialize");
        assert_eq!(yaml.trim(), "service: postgres");
        
        let deserialized: Dependency = serde_yaml::from_str(&yaml).expect("Failed to deserialize");
        assert_eq!(service_dep, deserialized);

        // Test task dependency
        let task_dep = Dependency::Task {
            task: "deploy-contracts".to_string(),
        };
        let yaml = serde_yaml::to_string(&task_dep).expect("Failed to serialize");
        assert_eq!(yaml.trim(), "task: deploy-contracts");
        
        let deserialized: Dependency = serde_yaml::from_str(&yaml).expect("Failed to deserialize");
        assert_eq!(task_dep, deserialized);
    }

    #[test]
    fn test_dependencies_in_yaml() {
        let yaml = r#"
dependencies:
  - service: postgres
  - service: redis
  - task: deploy-contracts
  - task: migrate-database
"#;
        
        #[derive(Deserialize)]
        struct TestConfig {
            dependencies: Vec<Dependency>,
        }
        
        let config: TestConfig = serde_yaml::from_str(yaml).expect("Failed to parse YAML");
        assert_eq!(config.dependencies.len(), 4);
        
        match &config.dependencies[0] {
            Dependency::Service { service } => assert_eq!(service, "postgres"),
            _ => panic!("Expected service dependency"),
        }
        
        match &config.dependencies[2] {
            Dependency::Task { task } => assert_eq!(task, "deploy-contracts"),
            _ => panic!("Expected task dependency"),
        }
    }

    #[test]
    fn test_new_service_target_variants() {
        // Test DockerAttach
        let docker_attach = ServiceTarget::DockerAttach {
            container: "existing-container".to_string(),
            env: HashMap::from([("KEY".to_string(), "value".to_string())]),
        };
        
        let yaml = serde_yaml::to_string(&docker_attach).expect("Failed to serialize");
        assert!(yaml.contains("type: docker-attach"));
        assert!(yaml.contains("container: existing-container"));
        
        // Test ProcessAttach
        let process_attach = ServiceTarget::ProcessAttach {
            pid: Some(1234),
            process_name: None,
            env: HashMap::new(),
        };
        
        let yaml = serde_yaml::to_string(&process_attach).expect("Failed to serialize");
        assert!(yaml.contains("type: process-attach"));
        assert!(yaml.contains("pid: 1234"));
        
        // Test Remote with process mode
        let remote = ServiceTarget::Remote {
            host: "example.com".to_string(),
            user: "ubuntu".to_string(),
            mode: RemoteMode::Process {
                binary: "myapp".to_string(),
                args: vec!["--port".to_string(), "8080".to_string()],
            },
            env: HashMap::new(),
        };
        
        let yaml = serde_yaml::to_string(&remote).expect("Failed to serialize");
        assert!(yaml.contains("type: remote"));
        assert!(yaml.contains("host: example.com"));
        assert!(yaml.contains("binary: myapp"));
    }
}
