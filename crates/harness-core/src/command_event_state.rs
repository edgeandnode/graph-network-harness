//! Combined event and state pattern for tasks that execute commands
//!
//! This module provides a unified way to track both state transitions and
//! command execution events for deployment tasks.

use crate::error::Error;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use smol::lock::RwLock;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

/// Events that can occur during command execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CommandEvent {
    /// Command is about to be executed
    CommandStarted {
        command: String,
        args: Vec<String>,
        environment: HashMap<String, String>,
    },
    /// Command produced output
    CommandOutput {
        stdout: Option<String>,
        stderr: Option<String>,
    },
    /// Command completed
    CommandCompleted { exit_code: i32, duration_ms: u64 },
    /// Command failed
    CommandFailed { error: String },
    /// Custom event specific to the task
    Custom {
        name: String,
        data: serde_json::Value,
    },
}

/// State of a command-executing task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum CommandTaskState {
    /// Task has not started
    NotStarted,
    /// Checking prerequisites
    CheckingPrerequisites,
    /// Preparing for execution
    Preparing,
    /// Executing commands
    Executing,
    /// Verifying results
    Verifying,
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed,
    /// Rolling back changes
    RollingBack,
}

impl fmt::Display for CommandTaskState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotStarted => write!(f, "not_started"),
            Self::CheckingPrerequisites => write!(f, "checking_prerequisites"),
            Self::Preparing => write!(f, "preparing"),
            Self::Executing => write!(f, "executing"),
            Self::Verifying => write!(f, "verifying"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::RollingBack => write!(f, "rolling_back"),
        }
    }
}

/// Context for command execution tasks
pub struct CommandTaskContext {
    /// Current state
    state: Arc<RwLock<CommandTaskState>>,
    /// Event log
    events: Arc<RwLock<Vec<CommandEvent>>>,
    /// Progress (0-100)
    progress: Arc<RwLock<u8>>,
    /// Task-specific data
    data: Arc<RwLock<serde_json::Value>>,
}

impl CommandTaskContext {
    /// Create a new command task context
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(CommandTaskState::NotStarted)),
            events: Arc::new(RwLock::new(Vec::new())),
            progress: Arc::new(RwLock::new(0)),
            data: Arc::new(RwLock::new(serde_json::json!({}))),
        }
    }

    /// Get the current state
    pub async fn state(&self) -> CommandTaskState {
        *self.state.read().await
    }

    /// Set the state
    pub async fn set_state(&self, state: CommandTaskState) {
        *self.state.write().await = state;
    }

    /// Add an event
    pub async fn add_event(&self, event: CommandEvent) {
        self.events.write().await.push(event);
    }

    /// Get all events
    pub async fn events(&self) -> Vec<CommandEvent> {
        self.events.read().await.clone()
    }

    /// Set progress
    pub async fn set_progress(&self, progress: u8) {
        *self.progress.write().await = progress.min(100);
    }

    /// Get progress
    pub async fn progress(&self) -> u8 {
        *self.progress.read().await
    }

    /// Set task data
    pub async fn set_data(&self, data: serde_json::Value) {
        *self.data.write().await = data;
    }

    /// Get task data
    pub async fn data(&self) -> serde_json::Value {
        self.data.read().await.clone()
    }

    /// Record a command start event
    pub async fn record_command_start(
        &self,
        command: String,
        args: Vec<String>,
        environment: HashMap<String, String>,
    ) {
        self.add_event(CommandEvent::CommandStarted {
            command,
            args,
            environment,
        })
        .await;
    }

    /// Record command output
    pub async fn record_command_output(&self, stdout: Option<String>, stderr: Option<String>) {
        self.add_event(CommandEvent::CommandOutput { stdout, stderr })
            .await;
    }

    /// Record command completion
    pub async fn record_command_completed(&self, exit_code: i32, duration_ms: u64) {
        self.add_event(CommandEvent::CommandCompleted {
            exit_code,
            duration_ms,
        })
        .await;
    }

    /// Record command failure
    pub async fn record_command_failed(&self, error: String) {
        self.add_event(CommandEvent::CommandFailed { error }).await;
    }

    /// Record a custom event
    pub async fn record_custom_event(&self, name: String, data: serde_json::Value) {
        self.add_event(CommandEvent::Custom { name, data }).await;
    }
}

impl Default for CommandTaskContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for tasks that execute commands and track events
#[async_trait]
pub trait CommandExecutingTask: Send + Sync {
    /// Get the task context
    fn context(&self) -> &CommandTaskContext;

    /// Check if prerequisites are met
    async fn check_prerequisites(&self) -> Result<bool, Error>;

    /// Prepare for execution
    async fn prepare(&self) -> Result<(), Error>;

    /// Execute the main commands
    async fn execute(&self) -> Result<(), Error>;

    /// Verify the results
    async fn verify(&self) -> Result<(), Error>;

    /// Rollback on failure
    async fn rollback(&self) -> Result<(), Error>;

    /// Run the complete task lifecycle
    async fn run(&self) -> Result<(), Error> {
        let ctx = self.context();

        // Check prerequisites
        ctx.set_state(CommandTaskState::CheckingPrerequisites).await;
        ctx.set_progress(10).await;

        if !self.check_prerequisites().await? {
            ctx.set_state(CommandTaskState::Failed).await;
            return Err(Error::validation("Prerequisites not met"));
        }

        // Prepare
        ctx.set_state(CommandTaskState::Preparing).await;
        ctx.set_progress(20).await;

        if let Err(e) = self.prepare().await {
            ctx.set_state(CommandTaskState::Failed).await;
            return Err(e);
        }

        // Execute
        ctx.set_state(CommandTaskState::Executing).await;
        ctx.set_progress(40).await;

        if let Err(e) = self.execute().await {
            // Try to rollback
            ctx.set_state(CommandTaskState::RollingBack).await;
            ctx.set_progress(50).await;

            if let Err(rollback_err) = self.rollback().await {
                ctx.record_command_failed(format!("Rollback failed: {}", rollback_err))
                    .await;
            }

            ctx.set_state(CommandTaskState::Failed).await;
            return Err(e);
        }

        // Verify
        ctx.set_state(CommandTaskState::Verifying).await;
        ctx.set_progress(80).await;

        if let Err(e) = self.verify().await {
            ctx.set_state(CommandTaskState::Failed).await;
            return Err(e);
        }

        // Complete
        ctx.set_state(CommandTaskState::Completed).await;
        ctx.set_progress(100).await;

        Ok(())
    }
}

/// Builder for command execution with event tracking
pub struct CommandBuilder {
    command: String,
    args: Vec<String>,
    environment: HashMap<String, String>,
    context: Arc<CommandTaskContext>,
}

impl CommandBuilder {
    /// Create a new command builder
    pub fn new(command: impl Into<String>, context: Arc<CommandTaskContext>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            environment: HashMap::new(),
            context,
        }
    }

    /// Add an argument
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Add multiple arguments
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(|s| s.into()));
        self
    }

    /// Set an environment variable
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.environment.insert(key.into(), value.into());
        self
    }

    /// Execute the command and track events
    pub async fn execute(self) -> Result<std::process::Output, Error> {
        use std::time::Instant;

        // Record start event
        self.context
            .record_command_start(
                self.command.clone(),
                self.args.clone(),
                self.environment.clone(),
            )
            .await;

        let start = Instant::now();

        // Build and execute command
        let mut cmd = smol::process::Command::new(&self.command);
        cmd.args(&self.args);
        for (key, value) in self.environment {
            cmd.env(key, value);
        }

        match cmd.output().await {
            Ok(output) => {
                let duration_ms = start.elapsed().as_millis() as u64;

                // Record output
                let stdout = if !output.stdout.is_empty() {
                    Some(String::from_utf8_lossy(&output.stdout).to_string())
                } else {
                    None
                };
                let stderr = if !output.stderr.is_empty() {
                    Some(String::from_utf8_lossy(&output.stderr).to_string())
                } else {
                    None
                };
                self.context.record_command_output(stdout, stderr).await;

                // Record completion
                let exit_code = output.status.code().unwrap_or(-1);
                self.context
                    .record_command_completed(exit_code, duration_ms)
                    .await;

                if output.status.success() {
                    Ok(output)
                } else {
                    Err(Error::action(format!(
                        "Command failed with exit code {}",
                        exit_code
                    )))
                }
            }
            Err(e) => {
                self.context.record_command_failed(e.to_string()).await;
                Err(Error::action(format!("Failed to execute command: {}", e)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[smol_potat::test]
    async fn test_command_task_context() {
        let ctx = CommandTaskContext::new();

        // Test initial state
        assert_eq!(ctx.state().await, CommandTaskState::NotStarted);
        assert_eq!(ctx.progress().await, 0);
        assert_eq!(ctx.events().await.len(), 0);

        // Test state transition
        ctx.set_state(CommandTaskState::Executing).await;
        assert_eq!(ctx.state().await, CommandTaskState::Executing);

        // Test progress
        ctx.set_progress(50).await;
        assert_eq!(ctx.progress().await, 50);

        // Test progress clamping
        ctx.set_progress(150).await;
        assert_eq!(ctx.progress().await, 100);

        // Test events
        ctx.record_command_start(
            "echo".to_string(),
            vec!["hello".to_string()],
            HashMap::new(),
        )
        .await;
        assert_eq!(ctx.events().await.len(), 1);

        ctx.record_command_output(Some("hello".to_string()), None)
            .await;
        assert_eq!(ctx.events().await.len(), 2);
    }

    #[smol_potat::test]
    async fn test_command_builder() {
        let ctx = Arc::new(CommandTaskContext::new());

        let builder = CommandBuilder::new("echo", ctx.clone())
            .arg("hello")
            .arg("world");

        // This would execute in a real environment
        // For testing, we just verify the builder constructs correctly
        assert_eq!(builder.command, "echo");
        assert_eq!(builder.args, vec!["hello", "world"]);
    }
}
