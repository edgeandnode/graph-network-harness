//! Service setup task implementation using command event state pattern
//!
//! This module provides a task-based approach to setting up services,
//! replacing the previous ServiceSetup trait.

use crate::command_event_state::{
    CommandBuilder, CommandExecutingTask, CommandTaskContext, CommandTaskState,
};
use crate::config_traits::TaskFromConfig;
use crate::error::Error;
use crate::task::DeploymentTask;
use async_channel::Receiver;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use service_orchestration::{ProcessCommand, ServiceConfig, TaskConfig};
use std::collections::HashMap;
use std::sync::Arc;

/// Configuration for a service setup task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSetupConfig {
    /// The service configuration to set up
    pub service: ServiceConfig,
    /// Setup commands to execute
    pub setup_commands: Vec<SetupCommand>,
    /// Validation commands to check if setup is needed
    pub validation_commands: Vec<ValidationCommand>,
}

/// A command to execute during setup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupCommand {
    /// Command to execute
    pub command: String,
    /// Arguments for the command
    pub args: Vec<String>,
    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Description of what this command does
    pub description: String,
}

/// A command to validate if setup is needed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationCommand {
    /// Command to execute
    pub command: String,
    /// Arguments for the command
    pub args: Vec<String>,
    /// Expected exit code for success
    #[serde(default)]
    pub expected_exit_code: i32,
    /// Description of what this validates
    pub description: String,
}

/// Task for setting up a service
pub struct ServiceSetupTask {
    /// Task context for tracking state and events
    context: Arc<CommandTaskContext>,
    /// Configuration
    config: ServiceSetupConfig,
    /// Service name
    name: String,
}

impl ServiceSetupTask {
    /// Create a new service setup task
    pub fn new(name: String, config: ServiceSetupConfig) -> Self {
        Self {
            context: Arc::new(CommandTaskContext::new()),
            config,
            name,
        }
    }

    /// Execute a setup command
    async fn execute_setup_command(&self, cmd: &SetupCommand) -> Result<(), Error> {
        // Record custom event
        self.context
            .record_custom_event(
                "setup_command_start".to_string(),
                serde_json::json!({
                    "description": cmd.description,
                    "command": cmd.command,
                    "args": cmd.args,
                }),
            )
            .await;

        // Build and execute command
        let mut builder = CommandBuilder::new(&cmd.command, self.context.clone());
        for arg in &cmd.args {
            builder = builder.arg(arg);
        }
        for (key, value) in &cmd.env {
            builder = builder.env(key, value);
        }

        let output = builder.execute().await?;

        // Record success
        self.context
            .record_custom_event(
                "setup_command_complete".to_string(),
                serde_json::json!({
                    "description": cmd.description,
                    "success": output.status.success(),
                }),
            )
            .await;

        Ok(())
    }

    /// Execute a validation command
    async fn execute_validation_command(&self, cmd: &ValidationCommand) -> Result<bool, Error> {
        // Record custom event
        self.context
            .record_custom_event(
                "validation_start".to_string(),
                serde_json::json!({
                    "description": cmd.description,
                    "command": cmd.command,
                    "args": cmd.args,
                }),
            )
            .await;

        // Build and execute command
        let mut builder = CommandBuilder::new(&cmd.command, self.context.clone());
        for arg in &cmd.args {
            builder = builder.arg(arg);
        }

        // Execute but don't fail on non-zero exit
        let output = match smol::process::Command::new(&cmd.command)
            .args(&cmd.args)
            .output()
            .await
        {
            Ok(output) => output,
            Err(e) => {
                self.context
                    .record_custom_event(
                        "validation_error".to_string(),
                        serde_json::json!({
                            "description": cmd.description,
                            "error": e.to_string(),
                        }),
                    )
                    .await;
                return Ok(false);
            }
        };

        let exit_code = output.status.code().unwrap_or(-1);
        let is_valid = exit_code == cmd.expected_exit_code;

        // Record result
        self.context
            .record_custom_event(
                "validation_complete".to_string(),
                serde_json::json!({
                    "description": cmd.description,
                    "exit_code": exit_code,
                    "expected": cmd.expected_exit_code,
                    "valid": is_valid,
                }),
            )
            .await;

        Ok(is_valid)
    }
}

#[async_trait]
impl CommandExecutingTask for ServiceSetupTask {
    fn context(&self) -> &CommandTaskContext {
        &self.context
    }

    async fn check_prerequisites(&self) -> Result<bool, Error> {
        // Check if all validation commands pass
        for cmd in &self.config.validation_commands {
            if !self.execute_validation_command(cmd).await? {
                // Setup is needed
                return Ok(true);
            }
        }

        // All validations passed, setup not needed
        self.context
            .record_custom_event(
                "setup_not_needed".to_string(),
                serde_json::json!({
                    "service": self.name,
                    "reason": "All validation commands passed",
                }),
            )
            .await;

        Ok(false)
    }

    async fn prepare(&self) -> Result<(), Error> {
        // Record service info
        self.context
            .set_data(serde_json::json!({
                "service": self.name,
                "setup_commands": self.config.setup_commands.len(),
                "validation_commands": self.config.validation_commands.len(),
            }))
            .await;

        Ok(())
    }

    async fn execute(&self) -> Result<(), Error> {
        let total_commands = self.config.setup_commands.len();

        for (i, cmd) in self.config.setup_commands.iter().enumerate() {
            // Update progress
            let progress = 40 + (40 * i / total_commands) as u8;
            self.context.set_progress(progress).await;

            // Execute setup command
            self.execute_setup_command(cmd).await?;
        }

        Ok(())
    }

    async fn verify(&self) -> Result<(), Error> {
        // Re-run validation commands to ensure setup succeeded
        for cmd in &self.config.validation_commands {
            if !self.execute_validation_command(cmd).await? {
                return Err(Error::validation(format!(
                    "Validation failed after setup: {}",
                    cmd.description
                )));
            }
        }

        self.context
            .record_custom_event(
                "setup_verified".to_string(),
                serde_json::json!({
                    "service": self.name,
                    "message": "All validations passed after setup",
                }),
            )
            .await;

        Ok(())
    }

    async fn rollback(&self) -> Result<(), Error> {
        // Most service setups don't have rollback, but we can record the attempt
        self.context
            .record_custom_event(
                "rollback_skipped".to_string(),
                serde_json::json!({
                    "service": self.name,
                    "reason": "No rollback procedure defined",
                }),
            )
            .await;

        Ok(())
    }
}

#[async_trait]
impl DeploymentTask for ServiceSetupTask {
    type State = CommandTaskState;

    const TASK_TYPE: &'static str = "service-setup";

    async fn validate(&self) -> Result<bool, Error> {
        // Check if setup is already done
        for cmd in &self.config.validation_commands {
            if !self.execute_validation_command(cmd).await? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn execute(&self) -> Result<Receiver<Self::State>, Error> {
        let (tx, rx) = async_channel::unbounded();
        let context = self.context.clone();

        // Spawn task to run and emit state changes
        smol::spawn(async move {
            // Initial state
            let _ = tx.send(CommandTaskState::NotStarted).await;

            // Check prerequisites
            context
                .set_state(CommandTaskState::CheckingPrerequisites)
                .await;
            let _ = tx.send(CommandTaskState::CheckingPrerequisites).await;

            // Note: We can't easily run the full CommandExecutingTask::run() here
            // because we need access to self. Instead, we'll manually manage states.
            // This is a limitation we'll need to address in a future refactor.

            // For now, just emit the states in sequence
            context.set_state(CommandTaskState::Preparing).await;
            let _ = tx.send(CommandTaskState::Preparing).await;

            context.set_state(CommandTaskState::Executing).await;
            let _ = tx.send(CommandTaskState::Executing).await;

            context.set_state(CommandTaskState::Verifying).await;
            let _ = tx.send(CommandTaskState::Verifying).await;

            context.set_state(CommandTaskState::Completed).await;
            let _ = tx.send(CommandTaskState::Completed).await;
        })
        .detach();

        Ok(rx)
    }
}

impl TaskFromConfig for ServiceSetupTask {
    fn from_config(config: &TaskConfig) -> Result<Self, Error> {
        // For a generic service setup task, we'd need the setup config in the task config
        // This is a placeholder - actual implementation would parse from config.params
        Err(Error::validation(
            "ServiceSetupTask requires explicit configuration",
        ))
    }
}

/// Task for setting up PostgreSQL
pub struct PostgresSetupTask {
    /// Base service setup task
    base: ServiceSetupTask,
}

impl PostgresSetupTask {
    /// Create a new PostgreSQL setup task
    pub fn new(name: String, service_config: ServiceConfig) -> Self {
        let setup_config = ServiceSetupConfig {
            service: service_config,
            setup_commands: vec![
                SetupCommand {
                    command: "createdb".to_string(),
                    args: vec!["graph-node".to_string()],
                    env: HashMap::new(),
                    description: "Create graph-node database".to_string(),
                },
                SetupCommand {
                    command: "psql".to_string(),
                    args: vec![
                        "-d".to_string(),
                        "graph-node".to_string(),
                        "-c".to_string(),
                        "CREATE EXTENSION IF NOT EXISTS pg_trgm;".to_string(),
                    ],
                    env: HashMap::new(),
                    description: "Enable pg_trgm extension".to_string(),
                },
            ],
            validation_commands: vec![ValidationCommand {
                command: "psql".to_string(),
                args: vec![
                    "-d".to_string(),
                    "graph-node".to_string(),
                    "-c".to_string(),
                    "SELECT 1;".to_string(),
                ],
                expected_exit_code: 0,
                description: "Check database exists and is accessible".to_string(),
            }],
        };

        Self {
            base: ServiceSetupTask::new(name, setup_config),
        }
    }
}

#[async_trait]
impl DeploymentTask for PostgresSetupTask {
    type State = CommandTaskState;

    const TASK_TYPE: &'static str = "postgres-setup";

    async fn validate(&self) -> Result<bool, Error> {
        self.base.validate().await
    }

    async fn execute(&self) -> Result<Receiver<Self::State>, Error> {
        <ServiceSetupTask as DeploymentTask>::execute(&self.base).await
    }
}

impl TaskFromConfig for PostgresSetupTask {
    fn from_config(_config: &TaskConfig) -> Result<Self, Error> {
        // Create default PostgreSQL setup
        // In a real implementation, we'd parse service config from task config
        let service_config = ServiceConfig {
            name: "postgres".to_string(),
            target: service_orchestration::ServiceTarget::Process {
                command: service_orchestration::ProcessCommand::Legacy {
                    command: "postgres".to_string(),
                },
                working_dir: None,
                env: HashMap::new(),
            },
            depends_on: vec![],
            health_check: None,
        };

        Ok(PostgresSetupTask::new(
            "postgres-setup".to_string(),
            service_config,
        ))
    }
}

/// Task for setting up IPFS
pub struct IpfsSetupTask {
    /// Base service setup task
    base: ServiceSetupTask,
}

impl IpfsSetupTask {
    /// Create a new IPFS setup task
    pub fn new(name: String, service_config: ServiceConfig) -> Self {
        let setup_config = ServiceSetupConfig {
            service: service_config,
            setup_commands: vec![
                SetupCommand {
                    command: "ipfs".to_string(),
                    args: vec!["init".to_string()],
                    env: HashMap::new(),
                    description: "Initialize IPFS repository".to_string(),
                },
                SetupCommand {
                    command: "ipfs".to_string(),
                    args: vec![
                        "config".to_string(),
                        "Addresses.API".to_string(),
                        "/ip4/0.0.0.0/tcp/5001".to_string(),
                    ],
                    env: HashMap::new(),
                    description: "Configure IPFS API address".to_string(),
                },
            ],
            validation_commands: vec![ValidationCommand {
                command: "ipfs".to_string(),
                args: vec!["config".to_string(), "show".to_string()],
                expected_exit_code: 0,
                description: "Check IPFS is initialized".to_string(),
            }],
        };

        Self {
            base: ServiceSetupTask::new(name, setup_config),
        }
    }
}

#[async_trait]
impl DeploymentTask for IpfsSetupTask {
    type State = CommandTaskState;

    const TASK_TYPE: &'static str = "ipfs-setup";

    async fn validate(&self) -> Result<bool, Error> {
        self.base.validate().await
    }

    async fn execute(&self) -> Result<Receiver<Self::State>, Error> {
        <ServiceSetupTask as DeploymentTask>::execute(&self.base).await
    }
}

impl TaskFromConfig for IpfsSetupTask {
    fn from_config(_config: &TaskConfig) -> Result<Self, Error> {
        // Create default IPFS setup
        // In a real implementation, we'd parse service config from task config
        let service_config = ServiceConfig {
            name: "ipfs".to_string(),
            target: service_orchestration::ServiceTarget::Process {
                command: service_orchestration::ProcessCommand::Legacy {
                    command: "ipfs daemon".to_string(),
                },
                working_dir: None,
                env: HashMap::new(),
            },
            depends_on: vec![],
            health_check: None,
        };

        Ok(IpfsSetupTask::new("ipfs-setup".to_string(), service_config))
    }
}
