//! Subgraph deployment using statig state machine
//!
//! This module implements a robust state machine for deploying subgraphs
//! to Graph Node with proper verification and error recovery using the statig crate.

// Allow missing docs for statig macro-generated code
#![allow(missing_docs)]

use async_trait::async_trait;
use command_executor::{
    Command, Executor, ProcessEventType, ProcessHandle, backends::LocalLauncher,
};
use futures::StreamExt;
use harness_core::{Error, config_traits::TaskFromConfig, task::DeploymentTask};
use schemars::JsonSchema;
use serde::Serialize;
use service_orchestration::{ServiceTarget, TaskConfig};
use statig::prelude::*;
use std::path::PathBuf;
use std::result::Result;
use tracing::{debug, error, info, warn};

/// Wrapper for subgraph deployment task
#[derive(Debug, Clone)]
pub struct SubgraphDeployTask {
    /// Graph Node endpoint URL
    graph_node_url: String,
    /// IPFS endpoint URL
    ipfs_url: String,
    /// Ethereum RPC URL
    ethereum_url: String,
    /// Working directory for subgraph
    working_dir: PathBuf,
    /// Subgraph name
    subgraph_name: String,
}

impl SubgraphDeployTask {
    /// Create a new subgraph deployment task
    pub fn new(
        graph_node_url: String,
        ipfs_url: String,
        ethereum_url: String,
        working_dir: String,
        subgraph_name: String,
    ) -> Self {
        Self {
            graph_node_url,
            ipfs_url,
            ethereum_url,
            working_dir: PathBuf::from(working_dir),
            subgraph_name,
        }
    }

    /// Run the deployment using the state machine (legacy method for compatibility)
    pub async fn deploy(&self) -> Result<(), Error> {
        deploy_subgraph(
            self.graph_node_url.clone(),
            self.ipfs_url.clone(),
            self.ethereum_url.clone(),
            self.working_dir.clone(),
            self.subgraph_name.clone(),
        )
        .await
    }
}

#[async_trait]
impl DeploymentTask for SubgraphDeployTask {
    type State = SubgraphDeployTaskState;

    const TASK_TYPE: &'static str = "subgraph-deployment";

    async fn execute(&self) -> Result<async_channel::Receiver<Self::State>, Error> {
        let (tx, rx) = async_channel::unbounded();

        // Clone what we need for the async task
        let graph_node_url = self.graph_node_url.clone();
        let ipfs_url = self.ipfs_url.clone();
        let ethereum_url = self.ethereum_url.clone();
        let working_dir = self.working_dir.clone();
        let subgraph_name = self.subgraph_name.clone();

        // Spawn the state machine execution
        smol::spawn(async move {
            // Send initial state
            let _ = tx
                .send(SubgraphDeployTaskState::CheckingPrerequisites)
                .await;

            // Run the deployment using the state machine
            match deploy_subgraph(
                graph_node_url,
                ipfs_url,
                ethereum_url,
                working_dir,
                subgraph_name,
            )
            .await
            {
                Ok(()) => {
                    let _ = tx.send(SubgraphDeployTaskState::Completed).await;
                }
                Err(e) => {
                    error!("Subgraph deployment failed: {}", e);
                    let _ = tx.send(SubgraphDeployTaskState::Failed).await;
                }
            }
        })
        .detach();

        Ok(rx)
    }
}

impl TaskFromConfig for SubgraphDeployTask {
    fn from_config(config: &TaskConfig) -> Result<Self, Error> {
        // Extract URLs from environment
        let graph_node_url = config
            .target
            .env()
            .get("GRAPH_NODE_URL")
            .cloned()
            .unwrap_or_else(|| "http://localhost:8020".to_string());

        let ipfs_url = config
            .target
            .env()
            .get("IPFS_URL")
            .cloned()
            .unwrap_or_else(|| "http://localhost:5001".to_string());

        let ethereum_url = config
            .target
            .env()
            .get("ETHEREUM_URL")
            .cloned()
            .unwrap_or_else(|| "http://localhost:8545".to_string());

        // Extract working directory from the Process variant or use default
        let working_dir = if let ServiceTarget::Process { working_dir, .. } = &config.target {
            working_dir
                .clone()
                .unwrap_or_else(|| "./subgraphs".to_string())
        } else {
            "./subgraphs".to_string()
        };

        // Extract subgraph name from params or use default
        let subgraph_name = config
            .target
            .get_param_str("subgraph_name")
            .unwrap_or_else(|| "test/subgraph".to_string());

        Ok(SubgraphDeployTask::new(
            graph_node_url,
            ipfs_url,
            ethereum_url,
            working_dir,
            subgraph_name,
        ))
    }
}

/// States for the subgraph deployment state machine
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub enum SubgraphDeployTaskState {
    /// Initial state - not started
    Idle,
    /// Checking if deployment is needed
    CheckingPrerequisites,
    /// Building the subgraph
    Building,
    /// Uploading to IPFS
    Uploading,
    /// Creating subgraph in Graph Node
    Creating,
    /// Deploying to Graph Node
    Deploying,
    /// Verifying deployment
    Verifying,
    /// Successfully completed
    Completed,
    /// Failed with error
    Failed,
}

/// Events that trigger state transitions
#[derive(Debug, Clone)]
pub enum SubgraphEvent {
    /// Start the deployment
    Start,
    /// Prerequisites checked - ready to proceed
    PrerequisitesReady,
    /// Already deployed - skip to completed
    AlreadyDeployed,
    /// Build completed
    BuildCompleted,
    /// Upload completed
    UploadCompleted,
    /// Subgraph created
    SubgraphCreated,
    /// Deployment completed
    DeploymentCompleted,
    /// Verification passed
    VerificationPassed,
    /// An error occurred
    Error(String),
    /// Retry the deployment
    Retry,
}

/// Shared context for the state machine
pub struct SubgraphContext {
    /// Graph Node endpoint URL
    pub graph_node_url: String,
    /// IPFS endpoint URL
    pub ipfs_url: String,
    /// Ethereum RPC URL
    pub ethereum_url: String,
    /// Working directory for subgraph
    pub working_dir: PathBuf,
    /// Subgraph name (e.g., "org/subgraph-name")
    pub subgraph_name: String,
    /// Command executor
    pub executor: Executor<LocalLauncher>,
    /// IPFS hash of the built subgraph
    pub ipfs_hash: Option<String>,
    /// GraphQL endpoints for the deployed subgraph
    pub endpoints: Vec<String>,
    /// Current progress (0-100)
    pub progress: u8,
    /// Status message
    pub status_message: String,
    /// Retry count
    pub retry_count: u32,
    /// Maximum retries
    pub max_retries: u32,
}

impl SubgraphContext {
    /// Create a new context
    pub fn new(
        graph_node_url: String,
        ipfs_url: String,
        ethereum_url: String,
        working_dir: PathBuf,
        subgraph_name: String,
    ) -> Self {
        Self {
            graph_node_url,
            ipfs_url,
            ethereum_url,
            working_dir,
            subgraph_name,
            executor: Executor::new("subgraph-deploy".to_string(), LocalLauncher),
            ipfs_hash: None,
            endpoints: Vec::new(),
            progress: 0,
            status_message: "Not started".to_string(),
            retry_count: 0,
            max_retries: 3,
        }
    }

    /// Update progress
    pub fn set_progress(&mut self, progress: u8, message: impl Into<String>) {
        self.progress = progress.min(100);
        self.status_message = message.into();
        info!(
            progress = self.progress,
            status = %self.status_message,
            "Subgraph deployment progress"
        );
    }

    /// Check if retries available
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }
}

/// State machine for subgraph deployment
pub struct SubgraphDeployTaskStateMachine {
    context: SubgraphContext,
}

impl SubgraphDeployTaskStateMachine {
    /// Create a new state machine
    pub fn new(context: SubgraphContext) -> Self {
        Self { context }
    }

    /// Check if subgraph already exists
    async fn check_subgraph_exists(context: &SubgraphContext) -> bool {
        let deployment_marker = context.working_dir.join(".deployment");
        deployment_marker.exists()
    }

    /// Verify working directory and required files exist
    fn verify_environment(context: &SubgraphContext) -> Result<(), Error> {
        if !context.working_dir.exists() {
            return Err(Error::daemon(format!(
                "Working directory does not exist: {}",
                context.working_dir.display()
            )));
        }

        let subgraph_yaml = context.working_dir.join("subgraph.yaml");
        if !subgraph_yaml.exists() {
            return Err(Error::daemon(
                "subgraph.yaml not found in working directory",
            ));
        }

        let package_json = context.working_dir.join("package.json");
        if !package_json.exists() {
            return Err(Error::daemon("package.json not found in working directory"));
        }

        Ok(())
    }

    /// Build the subgraph
    async fn build_subgraph(context: &mut SubgraphContext) -> Result<(), Error> {
        info!("Building subgraph");

        let mut cmd = Command::new("npx");
        cmd.args(["graph", "build"])
            .current_dir(&context.working_dir)
            .env("ETHEREUM_URL", &context.ethereum_url);

        let (mut event_stream, mut handle) = context
            .executor
            .launch(&command_executor::target::Target::Command, cmd)
            .await
            .map_err(|e| Error::daemon(format!("Failed to launch graph build: {e}")))?;

        while let Some(event) = event_stream.next().await {
            if let ProcessEventType::Stdout = &event.event_type {
                if let Some(data) = &event.data {
                    debug!("Graph build output: {}", data);

                    // Update progress based on output
                    if data.contains("Compile subgraph") {
                        context.set_progress(30, "Compiling subgraph");
                    } else if data.contains("Write compiled subgraph") {
                        context.set_progress(40, "Writing compiled subgraph");
                    }
                }
            }
        }

        // Wait for process to complete and check exit status
        let exit_status = handle
            .wait()
            .await
            .map_err(|e| Error::daemon(format!("Failed to wait for graph build: {e}")))?;

        if !exit_status.success() {
            return Err(Error::daemon("Subgraph build failed"));
        }

        Ok(())
    }

    /// Create the subgraph in Graph Node
    async fn create_subgraph(context: &mut SubgraphContext) -> Result<(), Error> {
        info!("Creating subgraph '{}'", context.subgraph_name);

        let mut cmd = Command::new("npx");
        cmd.args([
            "graph",
            "create",
            &context.subgraph_name,
            "--node",
            &context.graph_node_url,
        ])
        .current_dir(&context.working_dir);

        let (mut event_stream, mut handle) = context
            .executor
            .launch(&command_executor::target::Target::Command, cmd)
            .await
            .map_err(|e| Error::daemon(format!("Failed to launch graph create: {e}")))?;

        while let Some(event) = event_stream.next().await {
            if let ProcessEventType::Stdout = &event.event_type {
                if let Some(data) = &event.data {
                    debug!("Graph create output: {}", data);
                    // Subgraph creation is usually quick
                }
            }
        }

        // Wait for process to complete
        let exit_status = handle
            .wait()
            .await
            .map_err(|e| Error::daemon(format!("Failed to wait for graph create: {e}")))?;

        // Note: graph create may fail if subgraph already exists, which is okay
        if !exit_status.success() {
            warn!("Subgraph creation returned non-zero exit code (may already exist)");
        }

        Ok(())
    }

    /// Deploy the subgraph to Graph Node
    async fn deploy_subgraph(context: &mut SubgraphContext) -> Result<(), Error> {
        info!("Deploying subgraph '{}'", context.subgraph_name);

        let mut cmd = Command::new("npx");
        cmd.args([
            "graph",
            "deploy",
            &context.subgraph_name,
            "--node",
            &context.graph_node_url,
            "--ipfs",
            &context.ipfs_url,
            "--version-label",
            "v0.0.1",
        ])
        .current_dir(&context.working_dir)
        .env("ETHEREUM_URL", &context.ethereum_url);

        let (mut event_stream, mut handle) = context
            .executor
            .launch(&command_executor::target::Target::Command, cmd)
            .await
            .map_err(|e| Error::daemon(format!("Failed to launch graph deploy: {e}")))?;

        while let Some(event) = event_stream.next().await {
            if let ProcessEventType::Stdout = &event.event_type {
                if let Some(data) = &event.data {
                    debug!("Graph deploy output: {}", data);

                    // Extract IPFS hash
                    if data.contains("Build completed:") {
                        if let Some(hash) = extract_deployment_id(data) {
                            context.ipfs_hash = Some(hash.clone());
                            context.set_progress(70, format!("Build uploaded: {}", hash));
                        }
                    }

                    // Update progress
                    if data.contains("Upload subgraph to IPFS") {
                        context.set_progress(60, "Uploading to IPFS");
                    } else if data.contains("Deploy subgraph") {
                        context.set_progress(80, "Deploying to Graph Node");
                    } else if data.contains("Deployed to") {
                        context.set_progress(90, "Deployment complete");
                    }
                }
            }
        }

        // Wait for process to complete and check exit status
        let exit_status = handle
            .wait()
            .await
            .map_err(|e| Error::daemon(format!("Failed to wait for graph deploy: {e}")))?;

        if !exit_status.success() {
            return Err(Error::daemon("Subgraph deployment failed"));
        }

        // Save deployment info
        if let Some(ref hash) = context.ipfs_hash {
            context.endpoints = vec![
                format!(
                    "{}/subgraphs/name/{}",
                    context.graph_node_url, context.subgraph_name
                ),
                format!("{}/subgraphs/id/{}", context.graph_node_url, hash),
            ];

            let deployment_marker = context.working_dir.join(".deployment");
            let deployment_info = serde_json::json!({
                "name": context.subgraph_name,
                "ipfs_hash": hash,
                "endpoints": context.endpoints,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });
            let contents = serde_json::to_string_pretty(&deployment_info)
                .map_err(|e| Error::daemon(format!("Failed to serialize deployment info: {e}")))?;
            async_fs::write(&deployment_marker, contents)
                .await
                .map_err(|e| Error::daemon(format!("Failed to write deployment marker: {e}")))?;
        }

        Ok(())
    }

    /// Verify deployment succeeded
    fn verify_deployment(context: &SubgraphContext) -> Result<(), Error> {
        // Verify we have an IPFS hash
        if context.ipfs_hash.is_none() {
            return Err(Error::daemon("No IPFS hash found for deployed subgraph"));
        }

        // Verify we have endpoints
        if context.endpoints.is_empty() {
            return Err(Error::daemon("No endpoints found for deployed subgraph"));
        }

        // TODO: Could make an actual GraphQL query to verify the subgraph is responsive

        Ok(())
    }
}

/// State machine implementation for subgraph deployment
///
/// This state machine manages the lifecycle of deploying a subgraph to Graph Node:
/// - Idle: Initial state waiting for deployment to start
/// - CheckingPrerequisites: Verifying environment and checking if already deployed
/// - Building: Compiling the subgraph using graph-cli
/// - Creating: Creating the subgraph in Graph Node
/// - Deploying: Uploading to IPFS and deploying to Graph Node
/// - Verifying: Validating that deployment was successful
/// - Completed: Successfully finished deployment
/// - Failed: Deployment failed, may retry if under retry limit
#[statig::state_machine(initial = "State::idle()")]
impl SubgraphDeployTaskStateMachine {
    /// Initial idle state
    #[state]
    async fn idle(&mut self, event: &SubgraphEvent) -> Response<State> {
        info!("Idle {:?}", event);
        let context = &mut self.context;
        match event {
            SubgraphEvent::Start => {
                context.set_progress(5, "Starting subgraph deployment");
                Transition(State::checking_prerequisites())
            }
            _ => Super,
        }
    }

    /// Checking prerequisites state
    #[state]
    async fn checking_prerequisites(&mut self, event: &SubgraphEvent) -> Response<State> {
        info!("Checking prerequisites {:?}", event);
        let context = &mut self.context;
        context.set_progress(10, "Checking prerequisites");

        // Check if already deployed
        if Self::check_subgraph_exists(context).await {
            context.set_progress(100, "Subgraph already deployed");
            return Transition(State::completed());
        }

        // Verify environment
        if let Err(e) = Self::verify_environment(context) {
            context.set_progress(0, format!("Environment verification failed: {e}"));
            return Transition(State::failed());
        }

        context.set_progress(15, "Prerequisites verified");
        Transition(State::building())
    }

    /// Building subgraph state
    #[state]
    async fn building(&mut self, event: &SubgraphEvent) -> Response<State> {
        info!("Building {:?}", event);
        let context = &mut self.context;
        context.set_progress(20, "Building subgraph");

        if let Err(e) = Self::build_subgraph(context).await {
            context.set_progress(0, format!("Build failed: {e}"));
            if context.can_retry() {
                context.retry_count += 1;
                warn!(
                    "Retrying build (attempt {}/{})",
                    context.retry_count, context.max_retries
                );
                return Transition(State::building());
            }
            return Transition(State::failed());
        }

        context.set_progress(45, "Build completed");
        Transition(State::creating())
    }

    /// Creating subgraph state
    #[state]
    async fn creating(&mut self, event: &SubgraphEvent) -> Response<State> {
        info!("Creating {:?}", event);
        let context = &mut self.context;
        context.set_progress(50, "Creating subgraph in Graph Node");

        if let Err(e) = Self::create_subgraph(context).await {
            // Creation failure is often okay (subgraph may already exist)
            warn!("Subgraph creation warning: {e}");
        }

        context.set_progress(55, "Subgraph ready for deployment");
        Transition(State::deploying())
    }

    /// Deploying state
    #[state]
    async fn deploying(&mut self, event: &SubgraphEvent) -> Response<State> {
        info!("Deploying {:?}", event);
        let context = &mut self.context;
        context.set_progress(60, "Deploying subgraph");

        if let Err(e) = Self::deploy_subgraph(context).await {
            context.set_progress(0, format!("Deployment failed: {e}"));
            if context.can_retry() {
                context.retry_count += 1;
                warn!(
                    "Retrying deployment (attempt {}/{})",
                    context.retry_count, context.max_retries
                );
                return Transition(State::creating());
            }
            return Transition(State::failed());
        }

        context.set_progress(92, "Deployment completed");
        Transition(State::verifying())
    }

    /// Verifying deployment state
    #[state]
    async fn verifying(&mut self, event: &SubgraphEvent) -> Response<State> {
        info!("Verifying {:?}", event);
        let context = &mut self.context;
        context.set_progress(95, "Verifying deployment");

        if let Err(e) = Self::verify_deployment(context) {
            context.set_progress(0, format!("Verification failed: {e}"));
            if context.can_retry() {
                context.retry_count += 1;
                warn!(
                    "Verification failed, retrying (attempt {}/{})",
                    context.retry_count, context.max_retries
                );
                return Transition(State::deploying());
            }
            return Transition(State::failed());
        }

        context.set_progress(100, "Deployment verified");
        Transition(State::completed())
    }

    /// Completed state
    #[state]
    async fn completed(&mut self, event: &SubgraphEvent) -> Response<State> {
        info!("Completed {:?}", event);
        let context = &self.context;
        info!(
            "Subgraph '{}' deployment completed successfully",
            context.subgraph_name
        );
        if let Some(ref hash) = context.ipfs_hash {
            info!("IPFS hash: {}", hash);
            for endpoint in &context.endpoints {
                info!("Endpoint: {}", endpoint);
            }
        }
        Super
    }

    /// Failed state
    #[state]
    async fn failed(&mut self, event: &SubgraphEvent) -> Response<State> {
        info!("Failed {:?}", event);
        let context = &mut self.context;
        match event {
            SubgraphEvent::Retry => {
                if context.can_retry() {
                    context.retry_count += 1;
                    info!(
                        "Retrying subgraph deployment (attempt {}/{})",
                        context.retry_count, context.max_retries
                    );
                    context.set_progress(5, "Retrying deployment");
                    Transition(State::checking_prerequisites())
                } else {
                    error!("Retry limit exceeded for subgraph deployment");
                    Super
                }
            }
            _ => Super,
        }
    }
}

/// Extract deployment ID from graph CLI output
fn extract_deployment_id(line: &str) -> Option<String> {
    if line.contains("Build completed:") {
        if let Some(start) = line.find("Qm") {
            let hash: String = line[start..]
                .chars()
                .take_while(|c| c.is_alphanumeric())
                .collect();
            if hash.len() > 10 {
                return Some(hash);
            }
        }
    }
    None
}

/// Run the subgraph deployment
pub async fn deploy_subgraph(
    graph_node_url: String,
    ipfs_url: String,
    ethereum_url: String,
    working_dir: PathBuf,
    subgraph_name: String,
) -> Result<(), Error> {
    let context = SubgraphContext::new(
        graph_node_url,
        ipfs_url,
        ethereum_url,
        working_dir,
        subgraph_name,
    );
    let state_machine = SubgraphDeployTaskStateMachine::new(context);
    let mut machine = state_machine.state_machine();

    // Start the deployment
    machine.handle(&SubgraphEvent::Start).await;

    // Check final state
    match machine.state() {
        State::Completed {} => {
            info!("Subgraph deployment completed successfully");
            Ok(())
        }
        State::Failed {} => Err(Error::daemon("Subgraph deployment failed")),
        _ => Err(Error::daemon(
            "Subgraph deployment ended in unexpected state",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[smol_potat::test]
    async fn test_state_machine_initialization() {
        let context = SubgraphContext::new(
            "http://localhost:8020".to_string(),
            "http://localhost:5001".to_string(),
            "http://localhost:8545".to_string(),
            std::path::PathBuf::from("/tmp/test"),
            "test/subgraph".to_string(),
        );
        let state_machine = SubgraphDeployTaskStateMachine::new(context);
        let machine = state_machine.state_machine();

        // Initial state should be idle
        assert!(matches!(machine.state(), State::Idle {}));
    }

    #[smol_potat::test]
    async fn test_already_deployed_check() {
        let temp_dir = TempDir::new().unwrap();
        let marker = temp_dir.path().join(".deployment");
        async_fs::write(&marker, "{}").await.unwrap();

        let context = SubgraphContext::new(
            "http://localhost:8020".to_string(),
            "http://localhost:5001".to_string(),
            "http://localhost:8545".to_string(),
            temp_dir.path().to_path_buf(),
            "test/subgraph".to_string(),
        );

        // Just test the check function
        assert!(SubgraphDeployTaskStateMachine::check_subgraph_exists(&context).await);
    }

    #[test]
    fn test_extract_deployment_id() {
        let line = "Build completed: QmSubgraph123abc456def789";
        let result = extract_deployment_id(line);
        assert_eq!(result, Some("QmSubgraph123abc456def789".to_string()));
    }
}
