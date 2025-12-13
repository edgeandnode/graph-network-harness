//! Execution target types
//!
//! This module defines the various target types that can be executed by launchers.
//! Targets are location-agnostic - they define WHAT to execute, while launchers
//! determine WHERE to execute them.

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

// Note: Service attachment is handled at the service-orchestration layer
// using regular executors to run observation commands (status, logs, etc.)

// Note: Docker containers are handled at the service-orchestration layer:
// - ServiceTarget::Docker for creating new containers
// - ServiceTarget::DockerAttach for attaching to existing containers
// - LayerConfig::Docker for executing commands in containers
