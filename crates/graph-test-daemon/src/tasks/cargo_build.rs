//! Cargo build task using statig state machine
//!
//! Generic task for building Rust projects with cargo. Can be parameterized
//! to build different binaries (e.g., graph-node).

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
use statig::prelude::*;
use std::path::PathBuf;
use std::result::Result;
use tracing::{error, info};

// Implement PartialEq for CargoBuildTaskState manually since JsonSchema doesn't derive it
impl PartialEq for CargoBuildTaskState {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::CheckingPrerequisites, Self::CheckingPrerequisites) => true,
            (Self::Building { progress: a }, Self::Building { progress: b }) => a == b,
            (Self::Completed { outputs: a }, Self::Completed { outputs: b }) => {
                a.binary_path == b.binary_path && a.success == b.success
            }
            (Self::Failed { error: a }, Self::Failed { error: b }) => a == b,
            _ => false,
        }
    }
}

/// Cargo build task with configurable parameters
#[derive(Debug, Clone)]
pub struct CargoBuildTask {
    /// Source directory containing Cargo.toml
    source_dir: PathBuf,
    /// Binary name to build (optional, builds default if not specified)
    binary_name: Option<String>,
    /// Build in release mode
    release: bool,
}

impl CargoBuildTask {
    /// Create a new cargo build task
    pub fn new(source_dir: PathBuf, binary_name: Option<String>, release: bool) -> Self {
        Self {
            source_dir,
            binary_name,
            release,
        }
    }

    /// Get the expected output binary path
    pub fn binary_path(&self) -> PathBuf {
        let profile = if self.release { "release" } else { "debug" };
        let binary = self.binary_name.as_deref().unwrap_or("main");
        self.source_dir.join("target").join(profile).join(binary)
    }
}

impl TaskFromConfig for CargoBuildTask {
    fn from_config(config: &TaskConfig) -> Result<Self, Error> {
        // Extract source_dir from working_dir or config params
        let source_dir = if let ServiceTarget::Process { working_dir, .. } = &config.target {
            working_dir
                .clone()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
        } else {
            PathBuf::from(".")
        };

        // Extract binary_name from task config
        let binary_name = config
            .config
            .get("binary_name")
            .and_then(|v| v.as_str())
            .map(String::from);

        // Extract release mode from task config (default true)
        let release = config
            .config
            .get("release")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        Ok(CargoBuildTask::new(source_dir, binary_name, release))
    }
}

/// Build output containing the path to the built binary
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CargoBuildOutput {
    /// Path to the built binary
    pub binary_path: String,
    /// Whether the build succeeded
    pub success: bool,
}

/// State for the cargo build task state machine
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum CargoBuildTaskState {
    /// Checking if binary already exists and is up to date
    CheckingPrerequisites,
    /// Running cargo build
    Building { progress: u8 },
    /// Build completed successfully
    Completed { outputs: CargoBuildOutput },
    /// Build failed
    Failed { error: String },
}

/// Events that drive the cargo build state machine
#[derive(Debug, Clone)]
pub enum CargoBuildEvent {
    /// Binary already exists and is up to date
    AlreadyBuilt,
    /// Need to build
    NeedsBuild,
    /// Build progress update
    BuildProgress(u8),
    /// Build completed
    BuildComplete(String),
    /// Build failed
    BuildFailed(String),
}

/// Context for the cargo build state machine
pub struct CargoBuildContext {
    source_dir: PathBuf,
    binary_name: Option<String>,
    release: bool,
}

/// Cargo build state machine
#[derive(Default)]
pub struct CargoBuildTaskStateMachine;

#[state_machine(
    initial = "State::idle()",
    on_transition = "Self::on_transition",
    state(derive(Debug, Clone))
)]
impl CargoBuildTaskStateMachine {
    #[state]
    fn idle(event: &CargoBuildEvent) -> Response<State> {
        match event {
            CargoBuildEvent::AlreadyBuilt => Transition(State::completed()),
            CargoBuildEvent::NeedsBuild => Transition(State::building()),
            _ => Super,
        }
    }

    #[state]
    fn building(event: &CargoBuildEvent) -> Response<State> {
        match event {
            CargoBuildEvent::BuildComplete(_) => Transition(State::completed()),
            CargoBuildEvent::BuildFailed(_) => Transition(State::failed()),
            CargoBuildEvent::BuildProgress(_) => Handled,
            _ => Super,
        }
    }

    #[state]
    fn completed(event: &CargoBuildEvent) -> Response<State> {
        let _ = event;
        Handled
    }

    #[state]
    fn failed(event: &CargoBuildEvent) -> Response<State> {
        let _ = event;
        Handled
    }

    #[allow(unused_variables)]
    fn on_transition(&mut self, source: &State, target: &State) {
        info!("{:?} {:?}", source, target);
    }
}

#[async_trait]
impl DeploymentTask for CargoBuildTask {
    type State = CargoBuildTaskState;

    const TASK_TYPE: &'static str = "cargo-build";

    async fn execute(
        &self,
        _ctx: &service_orchestration::RuntimeContext,
    ) -> Result<async_channel::Receiver<Self::State>, Error> {
        let (tx, rx) = async_channel::unbounded();

        let source_dir = self.source_dir.clone();
        let binary_name = self.binary_name.clone();
        let release = self.release;
        let binary_path = self.binary_path();

        smol::spawn(async move {
            let _ = tx.send(CargoBuildTaskState::CheckingPrerequisites).await;

            match run_cargo_build(source_dir, binary_name, release, binary_path).await {
                Ok(output) => {
                    info!("Cargo build completed: {}", output.binary_path);
                    let _ = tx
                        .send(CargoBuildTaskState::Completed { outputs: output })
                        .await;
                }
                Err(e) => {
                    error!("Cargo build failed: {}", e);
                    let _ = tx
                        .send(CargoBuildTaskState::Failed {
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

/// Run the cargo build process
pub async fn run_cargo_build(
    source_dir: PathBuf,
    binary_name: Option<String>,
    release: bool,
    expected_binary: PathBuf,
) -> Result<CargoBuildOutput, Error> {
    // Check if binary already exists
    if expected_binary.exists() {
        info!("Binary already exists at {:?}", expected_binary);
        return Ok(CargoBuildOutput {
            binary_path: expected_binary.to_string_lossy().to_string(),
            success: true,
        });
    }

    info!("Building in {:?}", source_dir);

    // Build cargo command
    let mut cmd = Command::new("cargo");
    cmd.arg("build");
    if release {
        cmd.arg("--release");
    }
    if let Some(bin) = &binary_name {
        cmd.arg("--bin").arg(bin);
    }
    cmd.current_dir(&source_dir);

    let executor = Executor::new("cargo-build".to_string(), LocalLauncher);
    let (mut event_stream, mut handle) = executor
        .launch(&command_executor::target::Target::Command, cmd)
        .await
        .map_err(|e| Error::daemon(format!("Failed to spawn cargo: {}", e)))?;

    // Stream output - just log it
    let mut last_error = String::new();
    while let Some(event) = event_stream.next().await {
        match event.event_type {
            ProcessEventType::Stdout | ProcessEventType::Stderr => {
                if let Some(line) = &event.data {
                    // Log all output
                    info!("cargo: {}", line);
                    // Capture error lines for failure message
                    if line.contains("error") || line.contains("Error") {
                        last_error = line.clone();
                    }
                }
            }
            _ => {}
        }
    }

    // Wait for process to complete
    let exit_status = handle
        .wait()
        .await
        .map_err(|e| Error::daemon(format!("Failed to wait for cargo: {}", e)))?;

    if exit_status.success() {
        info!("Cargo build completed successfully");
        Ok(CargoBuildOutput {
            binary_path: expected_binary.to_string_lossy().to_string(),
            success: true,
        })
    } else {
        let error_msg = if last_error.is_empty() {
            format!("exit code {:?}", exit_status.code)
        } else {
            last_error
        };
        Err(Error::daemon(format!("Cargo build failed: {}", error_msg)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binary_path() {
        let task = CargoBuildTask::new(
            PathBuf::from("/home/user/graph-node"),
            Some("graph-node".to_string()),
            true,
        );
        assert_eq!(
            task.binary_path(),
            PathBuf::from("/home/user/graph-node/target/release/graph-node")
        );

        let debug_task = CargoBuildTask::new(
            PathBuf::from("/home/user/graph-node"),
            Some("graph-node".to_string()),
            false,
        );
        assert_eq!(
            debug_task.binary_path(),
            PathBuf::from("/home/user/graph-node/target/debug/graph-node")
        );
    }
}
