//! Execution target types
//! 
//! This module defines the various target types that can be executed by launchers.
//! Targets are location-agnostic - they define WHAT to execute, while launchers
//! determine WHERE to execute them.

use crate::command::Command;
use crate::error::Result;
use std::collections::HashMap;
use std::path::PathBuf;

/// Target types that can be executed by launchers
#[derive(Debug, Clone)]
pub enum Target {
    /// One-off command
    Command,
    /// Managed process
    ManagedProcess(ManagedProcess),
    /// Systemd service
    SystemdService(SystemdService),
    /// Systemd-portable service
    SystemdPortable(SystemdPortable),
    /// Docker container
    DockerContainer(DockerContainer),
    /// Docker compose service
    ComposeService(ComposeService),
}

// Individual target type structs

/// Execute as a managed process (we track PID and lifecycle)
#[derive(Debug, Clone)]
pub struct ManagedProcess {
    /// Optional process group ID for managing child processes
    process_group: Option<i32>,
    /// Whether to restart on failure
    restart_on_failure: bool,
}

impl ManagedProcess {
    /// Create a new managed process with default settings
    pub fn new() -> Self {
        Self {
            process_group: None,
            restart_on_failure: false,
        }
    }
    
    /// Create a builder for more complex configurations
    pub fn builder() -> ManagedProcessBuilder {
        ManagedProcessBuilder::new()
    }
    
    /// Set the process group ID
    pub fn with_process_group(mut self, pgid: i32) -> Self {
        self.process_group = Some(pgid);
        self
    }

    /// Enable restart on failure
    pub fn with_restart_on_failure(mut self) -> Self {
        self.restart_on_failure = true;
        self
    }
}

impl Default for ManagedProcess {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for ManagedProcess
pub struct ManagedProcessBuilder {
    process_group: Option<i32>,
    restart_on_failure: bool,
}

impl ManagedProcessBuilder {
    /// Create a new builder
    fn new() -> Self {
        Self {
            process_group: None,
            restart_on_failure: false,
        }
    }
    
    /// Set the process group ID
    pub fn process_group(mut self, pgid: i32) -> Self {
        self.process_group = Some(pgid);
        self
    }
    
    /// Enable restart on failure
    pub fn restart_on_failure(mut self, enabled: bool) -> Self {
        self.restart_on_failure = enabled;
        self
    }
    
    /// Build the ManagedProcess
    pub fn build(self) -> ManagedProcess {
        ManagedProcess {
            process_group: self.process_group,
            restart_on_failure: self.restart_on_failure,
        }
    }
}

/// Execute via systemd (systemctl commands)
#[derive(Debug, Clone)]
pub struct SystemdService {
    /// The systemd unit name
    unit_name: String,
}

impl SystemdService {
    /// Create a new systemd service target
    pub fn new(unit_name: impl Into<String>) -> Self {
        Self {
            unit_name: unit_name.into(),
        }
    }

    /// Get the unit name
    pub fn unit_name(&self) -> &str {
        &self.unit_name
    }
}

/// Execute via systemd-portable (portablectl commands)
#[derive(Debug, Clone)]
pub struct SystemdPortable {
    /// The portable service image name
    image_name: String,
    /// The systemd unit name
    unit_name: String,
}

impl SystemdPortable {
    /// Create a new systemd-portable service target
    pub fn new(image_name: impl Into<String>, unit_name: impl Into<String>) -> Self {
        Self {
            image_name: image_name.into(),
            unit_name: unit_name.into(),
        }
    }

    /// Get the image name
    pub fn image_name(&self) -> &str {
        &self.image_name
    }

    /// Get the unit name
    pub fn unit_name(&self) -> &str {
        &self.unit_name
    }
}

/// A generic managed service with configurable commands
#[derive(Debug, Clone)]
pub struct ManagedService {
    /// Service identifier
    name: String,
    /// How to check if service is running
    pub(crate) status_command: Command,
    /// How to start the service
    pub(crate) start_command: Command,
    /// How to stop the service
    pub(crate) stop_command: Command,
    /// How to restart the service (optional, will use stop+start if not provided)
    pub(crate) restart_command: Option<Command>,
    /// How to reload the service (optional)
    pub(crate) reload_command: Option<Command>,
    /// How to tail the logs
    pub(crate) log_command: Command,
}

impl ManagedService {
    /// Create a builder for a managed service
    pub fn builder(name: impl Into<String>) -> ManagedServiceBuilder {
        ManagedServiceBuilder::new(name)
    }

    /// Get the service name
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Builder for ManagedService
pub struct ManagedServiceBuilder {
    name: String,
    status_command: Option<Command>,
    start_command: Option<Command>,
    stop_command: Option<Command>,
    restart_command: Option<Command>,
    reload_command: Option<Command>,
    log_command: Option<Command>,
}

impl ManagedServiceBuilder {
    /// Create a new builder
    fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status_command: None,
            start_command: None,
            stop_command: None,
            restart_command: None,
            reload_command: None,
            log_command: None,
        }
    }

    /// Set the status command
    pub fn status_command(mut self, command: Command) -> Self {
        self.status_command = Some(command);
        self
    }

    /// Set the start command
    pub fn start_command(mut self, command: Command) -> Self {
        self.start_command = Some(command);
        self
    }

    /// Set the stop command
    pub fn stop_command(mut self, command: Command) -> Self {
        self.stop_command = Some(command);
        self
    }

    /// Set the restart command (optional)
    pub fn restart_command(mut self, command: Command) -> Self {
        self.restart_command = Some(command);
        self
    }

    /// Set the reload command (optional)
    pub fn reload_command(mut self, command: Command) -> Self {
        self.reload_command = Some(command);
        self
    }

    /// Set the log command
    pub fn log_command(mut self, command: Command) -> Self {
        self.log_command = Some(command);
        self
    }

    /// Build the ManagedService
    pub fn build(self) -> Result<ManagedService> {
        use crate::error::Error;
        Ok(ManagedService {
            name: self.name,
            status_command: self
                .status_command
                .ok_or_else(|| Error::spawn_failed("status_command is required"))?,
            start_command: self
                .start_command
                .ok_or_else(|| Error::spawn_failed("start_command is required"))?,
            stop_command: self
                .stop_command
                .ok_or_else(|| Error::spawn_failed("stop_command is required"))?,
            restart_command: self.restart_command,
            reload_command: self.reload_command,
            log_command: self
                .log_command
                .ok_or_else(|| Error::spawn_failed("log_command is required"))?,
        })
    }
}

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

// SSH support for ManagedService
#[cfg(feature = "ssh")]
impl crate::backends::ssh::SshTransformable for ManagedService {
    fn transform_for_ssh(&self, ssh_config: &crate::backends::ssh::SshConfig) -> Self {
        use crate::backends::ssh::wrap_command_with_ssh;
        
        Self {
            name: self.name.clone(),
            status_command: wrap_command_with_ssh(&self.status_command, ssh_config),
            start_command: wrap_command_with_ssh(&self.start_command, ssh_config),
            stop_command: wrap_command_with_ssh(&self.stop_command, ssh_config),
            restart_command: self.restart_command.as_ref()
                .map(|cmd| wrap_command_with_ssh(cmd, ssh_config)),
            reload_command: self.reload_command.as_ref()
                .map(|cmd| wrap_command_with_ssh(cmd, ssh_config)),
            log_command: wrap_command_with_ssh(&self.log_command, ssh_config),
        }
    }
}