//! Yarn task for Node.js projects
//!
//! Runs `yarn` in a directory to install dependencies or execute the default script.
//! For projects like indexer-agent where `yarn` is the standard build command.

#![allow(missing_docs)]

use async_trait::async_trait;
use command_executor::{
    Command, Executor, ProcessEventType, ProcessHandle, backends::LocalLauncher,
};
use futures::StreamExt;
use harness_core::{Error, config_traits::TaskFromConfig, task::DeploymentTask};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use service_orchestration::{ServiceTarget, TaskConfig};
use std::path::PathBuf;
use std::result::Result;
use tracing::{error, info};

/// Yarn task - runs `yarn` in a directory
#[derive(Debug, Clone)]
pub struct YarnTask {
    /// Working directory containing package.json
    working_dir: PathBuf,
    /// Optional script to run (e.g., "build"). If None, just runs `yarn`
    script: Option<String>,
    /// Skip if node_modules exists (for install-only tasks)
    skip_if_node_modules: bool,
}

impl YarnTask {
    /// Create a new yarn task
    pub fn new(working_dir: PathBuf, script: Option<String>, skip_if_node_modules: bool) -> Self {
        Self {
            working_dir,
            script,
            skip_if_node_modules,
        }
    }
}

impl TaskFromConfig for YarnTask {
    fn from_config(config: &TaskConfig) -> Result<Self, Error> {
        let working_dir = if let ServiceTarget::Process { working_dir, .. } = &config.target {
            working_dir
                .clone()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
        } else {
            PathBuf::from(".")
        };

        let script = config
            .config
            .get("script")
            .and_then(|v| v.as_str())
            .map(String::from);

        // Default: skip if node_modules exists (idempotent install)
        let skip_if_node_modules = config
            .config
            .get("skip_if_node_modules")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        Ok(YarnTask::new(working_dir, script, skip_if_node_modules))
    }
}

/// Output from yarn task
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct YarnOutput {
    /// Whether yarn succeeded
    pub success: bool,
    /// Working directory where yarn ran
    pub working_dir: String,
}

/// State for the yarn task
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum YarnTaskState {
    /// Checking if work is needed
    CheckingPrerequisites,
    /// Running yarn
    Running { progress: u8 },
    /// Completed successfully
    Completed { outputs: YarnOutput },
    /// Failed
    Failed { error: String },
}

impl PartialEq for YarnTaskState {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::CheckingPrerequisites, Self::CheckingPrerequisites) => true,
            (Self::Running { progress: a }, Self::Running { progress: b }) => a == b,
            (Self::Completed { outputs: a }, Self::Completed { outputs: b }) => {
                a.success == b.success && a.working_dir == b.working_dir
            }
            (Self::Failed { error: a }, Self::Failed { error: b }) => a == b,
            _ => false,
        }
    }
}

#[async_trait]
impl DeploymentTask for YarnTask {
    type State = YarnTaskState;

    const TASK_TYPE: &'static str = "yarn";

    async fn execute(
        &self,
        _ctx: &service_orchestration::RuntimeContext,
    ) -> Result<async_channel::Receiver<Self::State>, Error> {
        let (tx, rx) = async_channel::unbounded();

        let working_dir = self.working_dir.clone();
        let script = self.script.clone();
        let skip_if_node_modules = self.skip_if_node_modules;

        smol::spawn(async move {
            let _ = tx.send(YarnTaskState::CheckingPrerequisites).await;

            match run_yarn(working_dir, script, skip_if_node_modules).await {
                Ok(output) => {
                    info!("Yarn completed: {:?}", output.working_dir);
                    let _ = tx.send(YarnTaskState::Completed { outputs: output }).await;
                }
                Err(e) => {
                    error!("Yarn failed: {}", e);
                    let _ = tx
                        .send(YarnTaskState::Failed {
                            error: e.to_string(),
                        })
                        .await;
                }
            }
        })
        .detach();

        Ok(rx)
    }
}

/// Run yarn in the specified directory
pub async fn run_yarn(
    working_dir: PathBuf,
    script: Option<String>,
    skip_if_node_modules: bool,
) -> Result<YarnOutput, Error> {
    // Check if we can skip (node_modules exists and no specific script)
    if skip_if_node_modules && script.is_none() {
        let node_modules = working_dir.join("node_modules");
        if node_modules.exists() {
            info!(
                "node_modules exists at {:?}, skipping yarn install",
                working_dir
            );
            return Ok(YarnOutput {
                success: true,
                working_dir: working_dir.to_string_lossy().to_string(),
            });
        }
    }

    info!("Running yarn in {:?}", working_dir);

    let mut cmd = Command::new("yarn");
    if let Some(ref s) = script {
        cmd.arg(s);
    }
    cmd.current_dir(&working_dir);

    let executor = Executor::new("yarn".to_string(), LocalLauncher);
    let (mut event_stream, mut handle) = executor
        .launch(&command_executor::target::Target::Command, cmd)
        .await
        .map_err(|e| Error::daemon(format!("Failed to spawn yarn: {}", e)))?;

    let mut last_error = String::new();
    while let Some(event) = event_stream.next().await {
        match event.event_type {
            ProcessEventType::Stdout | ProcessEventType::Stderr => {
                if let Some(line) = &event.data {
                    info!("yarn: {}", line);
                    if line.contains("error") || line.contains("Error") {
                        last_error = line.clone();
                    }
                }
            }
            _ => {}
        }
    }

    let exit_status = handle
        .wait()
        .await
        .map_err(|e| Error::daemon(format!("Failed to wait for yarn: {}", e)))?;

    if exit_status.success() {
        info!("Yarn completed successfully");
        Ok(YarnOutput {
            success: true,
            working_dir: working_dir.to_string_lossy().to_string(),
        })
    } else {
        let error_msg = if last_error.is_empty() {
            format!("exit code {:?}", exit_status.code)
        } else {
            last_error
        };
        Err(Error::daemon(format!("Yarn failed: {}", error_msg)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yarn_task_creation() {
        let task = YarnTask::new(PathBuf::from("/app"), None, true);
        assert_eq!(task.working_dir, PathBuf::from("/app"));
        assert!(task.script.is_none());
        assert!(task.skip_if_node_modules);

        let build_task = YarnTask::new(PathBuf::from("/app"), Some("build".to_string()), false);
        assert_eq!(build_task.script, Some("build".to_string()));
        assert!(!build_task.skip_if_node_modules);
    }
}
