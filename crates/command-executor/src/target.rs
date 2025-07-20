//! Execution target types
//! 
//! This module defines the various target types that can be executed by launchers.
//! Targets are location-agnostic - they define WHAT to execute, while launchers
//! determine WHERE to execute them.

use std::collections::HashMap;
use std::path::PathBuf;

/// Configuration-level execution target specification
#[derive(Debug, Clone)]
pub enum ExecutionTarget {
    /// Execute as a one-off command
    Command,
    
    /// Execute as a managed process (we track PID and lifecycle)
    ManagedProcess {
        /// Optional process group ID
        process_group: Option<i32>,
        /// Whether to restart on failure
        restart_on_failure: bool,
    },
    
    /// Execute via systemd (systemctl commands)
    SystemdService {
        /// The systemd unit name
        unit_name: String,
    },
    
    /// Execute via systemd-portable (portablectl commands)
    SystemdPortable {
        /// The portable service image name
        image_name: String,
        /// The systemd unit name
        unit_name: String,
    },
    
    /// Execute inside a Docker container
    DockerContainer {
        /// Container ID or name
        container: String,
    },
    
    /// Execute as part of a docker-compose service
    ComposeService {
        /// Path to docker-compose.yml
        compose_file: PathBuf,
        /// Service name in the compose file
        service: String,
    },
}

// Individual target type structs

/// Docker container configuration
#[derive(Debug, Clone)]
pub struct DockerContainer {
    /// Docker image to run
    image: String,
    /// Optional container name
    name: Option<String>,
    /// Environment variables
    env: HashMap<String, String>,
    /// Volume mounts (host_path, container_path)
    volumes: Vec<(String, String)>,
    /// Working directory in container
    working_dir: Option<String>,
    /// Remove container on exit
    remove_on_exit: bool,
}

impl DockerContainer {
    /// Create a new Docker container configuration
    pub fn new(image: impl Into<String>) -> Self {
        Self {
            image: image.into(),
            name: None,
            env: HashMap::new(),
            volumes: Vec::new(),
            working_dir: None,
            remove_on_exit: true,
        }
    }

    /// Set container name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Add environment variable
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Add volume mount
    pub fn with_volume(mut self, host: impl Into<String>, container: impl Into<String>) -> Self {
        self.volumes.push((host.into(), container.into()));
        self
    }

    /// Set working directory
    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    /// Set whether to remove container on exit
    pub fn with_remove_on_exit(mut self, remove: bool) -> Self {
        self.remove_on_exit = remove;
        self
    }

    /// Get the image name
    pub fn image(&self) -> &str {
        &self.image
    }

    /// Get the container name
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Get environment variables
    pub fn env(&self) -> &HashMap<String, String> {
        &self.env
    }

    /// Get volume mounts
    pub fn volumes(&self) -> &[(String, String)] {
        &self.volumes
    }

    /// Get working directory
    pub fn working_dir(&self) -> Option<&str> {
        self.working_dir.as_deref()
    }

    /// Check if container should be removed on exit
    pub fn remove_on_exit(&self) -> bool {
        self.remove_on_exit
    }
}

/// Docker compose service configuration
#[derive(Debug, Clone)]
pub struct ComposeService {
    /// Path to docker-compose.yml file
    compose_file: PathBuf,
    /// Service name in the compose file
    service_name: String,
    /// Optional project name
    project_name: Option<String>,
}

impl ComposeService {
    /// Create a new compose service configuration
    pub fn new(compose_file: impl Into<PathBuf>, service_name: impl Into<String>) -> Self {
        Self {
            compose_file: compose_file.into(),
            service_name: service_name.into(),
            project_name: None,
        }
    }

    /// Set project name
    pub fn with_project_name(mut self, name: impl Into<String>) -> Self {
        self.project_name = Some(name.into());
        self
    }

    /// Get the compose file path
    pub fn compose_file(&self) -> &PathBuf {
        &self.compose_file
    }

    /// Get the service name
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    /// Get the project name
    pub fn project_name(&self) -> Option<&str> {
        self.project_name.as_deref()
    }
}