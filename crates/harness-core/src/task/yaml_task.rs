//! YAML-defined task implementation
//!
//! This provides a generic task that can be fully configured via YAML,
//! allowing users to define custom tasks without writing Rust code.

use super::DeploymentTask;
use crate::config_traits::TaskFromConfig;
use crate::error::Error;
use async_channel::Receiver;
use async_trait::async_trait;
use command_executor::{Command, ProcessHandle};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use service_orchestration::{RuntimeContext, ServiceTarget, TaskConfig};
use std::collections::HashMap;
use tracing::{debug, info};

/// State for YAML-defined tasks
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum YamlTaskState {
    /// Not started
    Idle,
    /// Checking if task needs to run
    Validating,
    /// Executing the task
    Running,
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed,
}

/// Configuration for a YAML-defined task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YamlTaskConfig {
    /// Optional validation command - if it succeeds, task is skipped
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_command: Option<String>,

    /// The actual command to execute
    pub command: String,

    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Working directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
}

/// A task that can be fully defined in YAML
#[derive(Clone)]
pub struct YamlTask {
    name: String,
    config: YamlTaskConfig,
    target: ServiceTarget,
}

impl YamlTask {
    /// Create a new YAML task
    pub fn new(name: String, config: YamlTaskConfig, target: ServiceTarget) -> Self {
        Self {
            name,
            config,
            target,
        }
    }

    /// Check if the task needs to run (validation)
    async fn validate_idempotent(&self) -> Result<bool, Error> {
        if let Some(validation_cmd) = &self.config.validation_command {
            debug!(
                "Running validation command for {}: {}",
                self.name, validation_cmd
            );

            // Use command-executor to run validation
            let mut cmd = Command::new("sh");
            cmd.arg("-c").arg(validation_cmd);

            // Add environment variables
            for (key, value) in &self.config.env {
                cmd.env(key, value);
            }

            // Set working directory if specified
            if let Some(dir) = &self.config.working_dir {
                cmd.current_dir(dir);
            }

            // Execute and check result
            match self.execute_cmd(cmd).await {
                Ok(mut handle) => {
                    let exit = handle.wait().await.map_err(|e| {
                        Error::action(format!("Failed to wait for validation command: {}", e))
                    })?;
                    // If validation succeeds, task is already done
                    Ok(exit.success())
                }
                Err(_) => {
                    // Validation failed, task needs to run
                    Ok(false)
                }
            }
        } else {
            // No validation command, always needs to run
            Ok(false)
        }
    }

    /// Execute a command using the appropriate backend
    async fn execute_cmd(&self, cmd: Command) -> Result<Box<dyn ProcessHandle>, Error> {
        match &self.target {
            ServiceTarget::Process { .. } => {
                // Local process execution
                use command_executor::backends::LocalLauncher;
                use command_executor::launcher::Launcher;
                use command_executor::target::Target;
                let launcher = LocalLauncher;
                let target = Target::Command;
                let (_events, handle) = launcher
                    .launch(&target, cmd)
                    .await
                    .map_err(|e| Error::action(format!("Failed to launch command: {}", e)))?;
                Ok(Box::new(handle))
            }
            ServiceTarget::Docker { .. } => {
                // Docker execution
                Err(Error::validation(
                    "Docker launcher not yet implemented for YAML tasks",
                ))
            }
            _ => Err(Error::validation("Unsupported target type for YAML task")),
        }
    }

    /// Execute the task command
    async fn execute_command(&self) -> Result<(), Error> {
        info!(
            "Executing YAML task '{}': {}",
            self.name, self.config.command
        );

        // Build command
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&self.config.command);

        // Add environment variables
        for (key, value) in &self.config.env {
            cmd.env(key, value);
        }

        // Set working directory
        if let Some(dir) = &self.config.working_dir {
            cmd.current_dir(dir);
        }

        // Execute
        let mut handle = self.execute_cmd(cmd).await?;

        let exit = handle
            .wait()
            .await
            .map_err(|e| Error::action(format!("Failed to wait for task command: {}", e)))?;

        if !exit.success() {
            return Err(Error::action(format!(
                "Task '{}' failed with exit code: {:?}",
                self.name, exit.code
            )));
        }

        Ok(())
    }
}

#[async_trait]
impl DeploymentTask for YamlTask {
    type State = YamlTaskState;

    const TASK_TYPE: &'static str = "yaml";

    async fn validate(&self) -> Result<bool, Error> {
        self.validate_idempotent().await
    }

    async fn execute(&self, _ctx: &RuntimeContext) -> Result<Receiver<Self::State>, Error> {
        let (tx, rx) = async_channel::unbounded();

        let name = self.name.clone();
        let config = self.config.clone();
        let target = self.target.clone();

        // Spawn execution task
        smol::spawn(async move {
            let task = YamlTask::new(name.clone(), config, target);

            // Initial state
            let _ = tx.send(YamlTaskState::Idle).await;

            // Check if already done
            let _ = tx.send(YamlTaskState::Validating).await;
            match task.validate_idempotent().await {
                Ok(true) => {
                    info!(
                        "Task '{}' already completed (validation passed), skipping",
                        name
                    );
                    let _ = tx.send(YamlTaskState::Completed).await;
                    return;
                }
                Ok(false) => {
                    debug!("Task '{}' needs to run", name);
                }
                Err(e) => {
                    info!("Task '{}' validation error (will run anyway): {}", name, e);
                }
            }

            // Execute the task
            let _ = tx.send(YamlTaskState::Running).await;
            match task.execute_command().await {
                Ok(()) => {
                    info!("Task '{}' completed successfully", name);
                    let _ = tx.send(YamlTaskState::Completed).await;
                }
                Err(e) => {
                    info!("Task '{}' failed: {}", name, e);
                    let _ = tx.send(YamlTaskState::Failed).await;
                }
            }
        })
        .detach();

        Ok(rx)
    }
}

impl TaskFromConfig for YamlTask {
    fn from_config(config: &TaskConfig) -> Result<Self, Error> {
        // Get command from command_template or command field
        let command = match &config.target {
            ServiceTarget::Process { command, .. } => {
                // For Process target, get command from ProcessCommand
                match command {
                    service_orchestration::ProcessCommand::Template {
                        command_template,
                        params,
                        ..
                    } => {
                        // Substitute params in command template
                        let mut cmd = command_template.clone();
                        for (key, value) in params {
                            let placeholder = format!("{{{}}}", key);
                            cmd = cmd.replace(&placeholder, &value.as_string());
                        }
                        cmd
                    }
                    service_orchestration::ProcessCommand::Legacy { command } => command.clone(),
                    service_orchestration::ProcessCommand::Typed { .. } => {
                        return Err(Error::validation(
                            "Typed tasks must use TypedTaskProvider, not YamlTask",
                        ));
                    }
                }
            }
            _ => {
                return Err(Error::validation(
                    "YAML task requires command or command_template",
                ));
            }
        };

        // Get complete_if and working_dir from the target config
        // These should be top-level fields in ServiceTarget, not in params
        let validation_command = match &config.target {
            ServiceTarget::Process { complete_if, .. } => complete_if.clone(),
            _ => None,
        };

        let working_dir = match &config.target {
            ServiceTarget::Process { working_dir, .. } => working_dir.clone(),
            _ => None,
        };

        let yaml_config = YamlTaskConfig {
            validation_command,
            command,
            env: config.target.env().clone(),
            working_dir,
        };

        Ok(YamlTask::new(
            "yaml-task".to_string(), // Name will come from the YAML key
            yaml_config,
            config.target.clone(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yaml_task_config() {
        let config = YamlTaskConfig {
            validation_command: Some("test -f /tmp/marker".to_string()),
            command: "touch /tmp/marker".to_string(),
            env: HashMap::from([("FOO".to_string(), "bar".to_string())]),
            working_dir: Some("/tmp".to_string()),
        };

        // Should serialize/deserialize correctly
        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("validation_command"));
        assert!(yaml.contains("touch /tmp/marker"));

        let parsed: YamlTaskConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.command, config.command);
    }
}
