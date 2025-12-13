//! Process-based task executor

use crate::{OrchestrationError, ServiceTarget, TaskConfig, TaskExecutor};
use async_channel::Receiver;
use async_runtime_compat::Spawner;
use async_trait::async_trait;
use command_executor::{Command, Launcher, ProcessEventType, ProcessHandle, Target};
use futures::StreamExt;
use serde_json::Value as JsonValue;
use std::result::Result;
use tracing::{debug, error, info};

/// Task executor for process-based tasks
pub struct ProcessTaskExecutor {}

impl ProcessTaskExecutor {
    /// Create a new process task executor
    pub fn new() -> Self {
        Self {}
    }

    /// Run complete_if command to check if task is already complete
    async fn check_complete(&self, config: &TaskConfig) -> Result<bool, OrchestrationError> {
        if let ServiceTarget::Process {
            complete_if: Some(check_cmd),
            env,
            working_dir,
            ..
        } = &config.target
        {
            debug!("Running complete_if check: {}", check_cmd);

            // Create the check command
            let mut cmd = Command::new("sh");
            cmd.arg("-c").arg(check_cmd);

            // Add environment variables
            for (key, value) in env {
                cmd.env(key, value);
            }

            // Set working directory if specified
            if let Some(dir) = working_dir {
                cmd.current_dir(dir);
            }

            // Execute and check result
            let launcher = command_executor::backends::LocalLauncher;
            let target = Target::Command;
            match launcher.launch(&target, cmd).await {
                Ok((_stream, mut handle)) => {
                    let exit = handle.wait().await.map_err(|e| {
                        OrchestrationError::Config(format!("Failed to wait for complete_if: {}", e))
                    })?;
                    // If check succeeds (exit code 0), task is already complete
                    Ok(exit.success())
                }
                Err(e) => {
                    debug!("complete_if check failed: {}", e);
                    // If check fails, task needs to run
                    Ok(false)
                }
            }
        } else {
            // No complete_if command, assume task needs to run
            Ok(false)
        }
    }
}

#[async_trait]
impl TaskExecutor for ProcessTaskExecutor {
    fn can_handle(&self, config: &TaskConfig) -> bool {
        matches!(config.target, ServiceTarget::Process { .. })
    }

    async fn is_complete(
        &self,
        _name: &str,
        config: &TaskConfig,
    ) -> Result<bool, OrchestrationError> {
        // Check if there's a complete_if command in the config
        self.check_complete(config).await
    }

    async fn execute(
        &self,
        name: &str,
        config: &TaskConfig,
        spawner: &dyn Spawner,
    ) -> Result<Receiver<JsonValue>, OrchestrationError> {
        info!("Executing process task: {}", name);

        // Extract command from config
        let (command_str, env, working_dir) = match &config.target {
            ServiceTarget::Process {
                command,
                env,
                working_dir,
                ..
            } => {
                let cmd_str = match command {
                    crate::config::ProcessCommand::Legacy { command } => command.clone(),
                    crate::config::ProcessCommand::Template {
                        command_template, ..
                    } => command_template.clone(),
                    crate::config::ProcessCommand::Typed { .. } => {
                        // Typed tasks are handled by TypedTaskProvider, not ProcessTaskExecutor
                        return Err(OrchestrationError::Config(
                            "Typed tasks must be executed via TypedTaskProvider".to_string(),
                        ));
                    }
                };
                (cmd_str, env.clone(), working_dir.clone())
            }
            _ => {
                return Err(OrchestrationError::Config(format!(
                    "ProcessTaskExecutor cannot handle non-process target"
                )));
            }
        };

        // Create channel for state updates
        let (tx, rx) = async_channel::unbounded();

        // Clone what we need for the spawned task
        let task_name = name.to_string();
        let tx_clone = tx.clone();

        // Spawn the task execution
        spawner.spawn_detached(Box::pin(async move {
            // Send initial state
            let _ = tx_clone.send(serde_json::json!("Running")).await;

            // Create and execute the command
            let mut cmd = Command::new("sh");
            cmd.arg("-c").arg(&command_str);

            // Add environment variables
            for (key, value) in &env {
                cmd.env(key, value);
            }

            // Set working directory if specified
            if let Some(dir) = &working_dir {
                cmd.current_dir(dir);
            }

            // Execute the command
            let launcher = command_executor::backends::LocalLauncher;
            let target = Target::Command;
            match launcher.launch(&target, cmd).await {
                Ok((mut event_stream, _handle)) => {
                    // Stream events from the process
                    while let Some(event) = event_stream.next().await {
                        match event.event_type {
                            ProcessEventType::Stdout => {
                                if let Some(ref data) = event.data {
                                    debug!("Task {} stdout: {}", task_name, data);
                                }
                            }
                            ProcessEventType::Stderr => {
                                if let Some(ref data) = event.data {
                                    debug!("Task {} stderr: {}", task_name, data);
                                }
                            }
                            ProcessEventType::Exited { code, .. } => {
                                if code == Some(0) {
                                    info!("Task {} completed successfully", task_name);
                                    let _ = tx_clone.send(serde_json::json!("Completed")).await;
                                } else {
                                    error!("Task {} failed with exit code: {:?}", task_name, code);
                                    let _ = tx_clone.send(serde_json::json!("Failed")).await;
                                }
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to execute task {}: {}", task_name, e);
                    let _ = tx_clone.send(serde_json::json!("Failed")).await;
                }
            }
        }));

        Ok(rx)
    }
}
