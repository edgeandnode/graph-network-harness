//! Execution target types
//!
//! This module defines the various target types that can be executed by launchers.
//! Targets are location-agnostic - they define WHAT to execute, while launchers
//! determine WHERE to execute them.

use crate::command::Command;
use crate::error::Error;
use std::result::Result;

/// Target types that can be executed by launchers
#[derive(Debug, Clone)]
pub enum Target {
    /// One-off command
    Command,
    /// Managed process
    ManagedProcess(ManagedProcess),
    // Note: Other target types (Docker, Systemd, etc.) are handled at the
    // service-orchestration layer or via the layered executor system
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

// Note: Systemd services are handled at the service-orchestration layer
// using SystemdAttachedExecutor for existing services

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
    pub fn build(self) -> Result<ManagedService, Error> {
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

// Note: Docker containers are handled at the service-orchestration layer:
// - ServiceTarget::Docker for creating new containers
// - ServiceTarget::DockerAttach for attaching to existing containers
// - LayerConfig::Docker for executing commands in containers

/// A service that can be observed but not controlled
#[derive(Debug, Clone)]
pub struct AttachedService {
    /// Service identifier
    name: String,
    /// How to check if service is running
    pub(crate) status_command: Command,
    /// How to tail the logs
    pub(crate) log_command: Command,
}

impl AttachedService {
    /// Create a builder for an attached service
    pub fn builder(name: impl Into<String>) -> AttachedServiceBuilder {
        AttachedServiceBuilder::new(name)
    }

    /// Get the service name
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Builder for AttachedService
pub struct AttachedServiceBuilder {
    name: String,
    status_command: Option<Command>,
    log_command: Option<Command>,
}

impl AttachedServiceBuilder {
    /// Create a new builder
    fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status_command: None,
            log_command: None,
        }
    }

    /// Set the status command
    pub fn status_command(mut self, command: Command) -> Self {
        self.status_command = Some(command);
        self
    }

    /// Set the log command
    pub fn log_command(mut self, command: Command) -> Self {
        self.log_command = Some(command);
        self
    }

    /// Build the AttachedService
    pub fn build(self) -> Result<AttachedService, Error> {
        Ok(AttachedService {
            name: self.name,
            status_command: self
                .status_command
                .ok_or_else(|| Error::spawn_failed("status_command is required"))?,
            log_command: self
                .log_command
                .ok_or_else(|| Error::spawn_failed("log_command is required"))?,
        })
    }
}
