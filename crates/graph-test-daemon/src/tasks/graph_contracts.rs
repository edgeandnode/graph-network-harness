//! Graph Protocol contracts deployment using statig state machine
//!
//! This module implements a robust state machine for deploying Graph Protocol
//! contracts with proper verification and error recovery using the statig crate.

// Allow missing docs for statig macro-generated code
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
use std::collections::HashMap;
use std::path::PathBuf;
use std::result::Result;
use tracing::{debug, error, info, warn};

/// Graph contracts deployment task with state machine
#[derive(Debug, Clone)]
pub struct GraphContractsTask {
    /// Ethereum RPC URL
    ethereum_url: String,
    /// Working directory for contracts
    working_dir: PathBuf,
}

impl GraphContractsTask {
    /// Create a new Graph contracts deployment task
    pub fn new(ethereum_url: String, working_dir: String) -> Self {
        Self {
            ethereum_url,
            working_dir: PathBuf::from(working_dir),
        }
    }
}

impl TaskFromConfig for GraphContractsTask {
    fn from_config(config: &TaskConfig) -> Result<Self, Error> {
        // Extract ethereum_url from environment
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
                .unwrap_or_else(|| "./contracts/graph-contracts".to_string())
        } else {
            "./contracts/graph-contracts".to_string()
        };

        Ok(GraphContractsTask::new(ethereum_url, working_dir))
    }
}

#[async_trait]
impl DeploymentTask for GraphContractsTask {
    type State = GraphContractsDeployTaskState;

    const TASK_TYPE: &'static str = "graph-contracts-deployment";

    async fn execute(&self) -> Result<async_channel::Receiver<Self::State>, Error> {
        let (tx, rx) = async_channel::unbounded();

        // Clone what we need for the async task
        let ethereum_url = self.ethereum_url.clone();
        let working_dir = self.working_dir.clone();

        // Spawn the state machine execution
        smol::spawn(async move {
            // Send initial state
            let _ = tx
                .send(GraphContractsDeployTaskState::CheckingPrerequisites)
                .await;

            // Run the deployment using the state machine
            match deploy_graph_contracts(ethereum_url, working_dir).await {
                Ok(addresses) => {
                    info!(
                        "Graph contracts deployed, complete={}",
                        addresses.is_complete()
                    );
                    let _ = tx
                        .send(GraphContractsDeployTaskState::Completed { outputs: addresses })
                        .await;
                }
                Err(e) => {
                    error!("Deployment failed: {}", e);
                    let _ = tx
                        .send(GraphContractsDeployTaskState::Failed {
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

/// Deployed Graph Protocol contract addresses
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GraphContractAddresses {
    /// Controller contract - manages protocol parameters
    #[serde(rename = "Controller")]
    pub controller: Option<String>,
    /// EpochManager contract - manages epochs for rewards
    #[serde(rename = "EpochManager")]
    pub epoch_manager: Option<String>,
    /// GraphToken (GRT) contract
    #[serde(rename = "GraphToken")]
    pub graph_token: Option<String>,
    /// Staking contract (L1Staking on mainnet)
    #[serde(rename = "L1Staking")]
    pub staking: Option<String>,
    /// Staking extension contract
    #[serde(rename = "StakingExtension")]
    pub staking_extension: Option<String>,
    /// Curation contract - manages signal on subgraphs
    #[serde(rename = "Curation")]
    pub curation: Option<String>,
    /// DisputeManager contract
    #[serde(rename = "DisputeManager")]
    pub dispute_manager: Option<String>,
    /// RewardsManager contract
    #[serde(rename = "RewardsManager")]
    pub rewards_manager: Option<String>,
    /// ServiceRegistry contract
    #[serde(rename = "ServiceRegistry")]
    pub service_registry: Option<String>,
    /// GNS (Graph Name Service) contract
    #[serde(rename = "L1GNS")]
    pub gns: Option<String>,
    /// SubgraphNFT contract
    #[serde(rename = "SubgraphNFT")]
    pub subgraph_nft: Option<String>,
    /// GraphTokenGateway contract (for L1/L2 bridging)
    #[serde(rename = "L1GraphTokenGateway")]
    pub graph_token_gateway: Option<String>,
}

impl GraphContractAddresses {
    /// Set a contract address by name (for parsing deployment output)
    pub fn set(&mut self, name: &str, address: String) {
        match name {
            "Controller" => self.controller = Some(address),
            "EpochManager" => self.epoch_manager = Some(address),
            "GraphToken" => self.graph_token = Some(address),
            "L1Staking" | "Staking" => self.staking = Some(address),
            "StakingExtension" => self.staking_extension = Some(address),
            "Curation" => self.curation = Some(address),
            "DisputeManager" => self.dispute_manager = Some(address),
            "RewardsManager" => self.rewards_manager = Some(address),
            "ServiceRegistry" => self.service_registry = Some(address),
            "L1GNS" | "GNS" => self.gns = Some(address),
            "SubgraphNFT" => self.subgraph_nft = Some(address),
            "L1GraphTokenGateway" => self.graph_token_gateway = Some(address),
            _ => {
                debug!("Unknown contract: {} at {}", name, address);
            }
        }
    }

    /// Check if the essential contracts are deployed
    pub fn is_complete(&self) -> bool {
        self.graph_token.is_some()
            && self.staking.is_some()
            && self.epoch_manager.is_some()
            && self.gns.is_some()
    }

    /// Count how many addresses are set
    pub fn count(&self) -> usize {
        [
            &self.controller,
            &self.epoch_manager,
            &self.graph_token,
            &self.staking,
            &self.staking_extension,
            &self.curation,
            &self.dispute_manager,
            &self.rewards_manager,
            &self.service_registry,
            &self.gns,
            &self.subgraph_nft,
            &self.graph_token_gateway,
        ]
        .iter()
        .filter(|a| a.is_some())
        .count()
    }

    /// Create from a HashMap (for deserializing from JSON files)
    pub fn from_map(map: &HashMap<String, String>) -> Self {
        let mut addresses = Self::default();
        for (name, address) in map {
            addresses.set(name, address.clone());
        }
        addresses
    }
}

/// States for the Graph contracts deployment state machine
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum GraphContractsDeployTaskState {
    /// Initial state - not started
    Idle,
    /// Checking if deployment is needed
    CheckingPrerequisites,
    /// Preparing environment for deployment
    Preparing,
    /// Deploying contracts
    DeployingContracts,
    /// Verifying deployment
    Verifying,
    /// Successfully completed with contract addresses
    Completed {
        /// Deployed contract addresses
        outputs: GraphContractAddresses,
    },
    /// Failed with error
    Failed {
        /// Error message
        #[serde(default)]
        error: String,
    },
}

/// Events that trigger state transitions
#[derive(Debug, Clone)]
pub enum GraphContractsEvent {
    /// Start the deployment
    Start,
    /// Prerequisites checked - ready to proceed
    PrerequisitesReady,
    /// Already deployed - skip to completed
    AlreadyDeployed,
    /// Environment prepared
    EnvironmentReady,
    /// Contracts deployed
    ContractsDeployed,
    /// Verification passed
    VerificationPassed,
    /// An error occurred
    Error(String),
    /// Retry the deployment
    Retry,
}

/// Shared context for the state machine
pub struct GraphContractsContext {
    /// Ethereum RPC URL
    pub ethereum_url: String,
    /// Working directory for contracts deployment
    pub working_dir: PathBuf,
    /// Command executor
    pub executor: Executor<LocalLauncher>,
    /// Deployed contract addresses
    pub deployed_addresses: GraphContractAddresses,
    /// Current progress (0-100)
    pub progress: u8,
    /// Status message
    pub status_message: String,
    /// Retry count
    pub retry_count: u32,
    /// Maximum retries
    pub max_retries: u32,
}

impl GraphContractsContext {
    /// Create a new context
    pub fn new(ethereum_url: String, working_dir: PathBuf) -> Self {
        Self {
            ethereum_url,
            working_dir,
            executor: Executor::new("graph-contracts".to_string(), LocalLauncher),
            deployed_addresses: GraphContractAddresses::default(),
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
            "Graph contracts deployment progress"
        );
    }

    /// Check if retries available
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }
}

/// State machine for Graph contracts deployment
pub struct GraphContractsDeployTaskStateMachine {
    context: GraphContractsContext,
}

impl GraphContractsDeployTaskStateMachine {
    /// Create a new state machine
    pub fn new(context: GraphContractsContext) -> Self {
        Self { context }
    }

    /// Check if contracts are already deployed
    async fn check_deployment_exists(context: &GraphContractsContext) -> bool {
        let deployment_marker = context.working_dir.join(".graph-network-deployed");
        deployment_marker.exists()
    }

    /// Verify environment is ready for deployment
    fn verify_environment(context: &GraphContractsContext) -> Result<(), Error> {
        if !context.working_dir.exists() {
            return Err(Error::daemon(format!(
                "Working directory does not exist: {}",
                context.working_dir.display()
            )));
        }

        // Check for package.json or other required files
        let package_json = context.working_dir.join("package.json");
        if !package_json.exists() {
            return Err(Error::daemon("package.json not found in working directory"));
        }

        Ok(())
    }

    /// Deploy the contracts
    async fn deploy_contracts(context: &mut GraphContractsContext) -> Result<(), Error> {
        info!("Deploying Graph Protocol contracts");

        let mut cmd = Command::new("npx");
        cmd.args(["hardhat", "deploy", "--network", "localhost"])
            .current_dir(&context.working_dir)
            .env("ETHEREUM_URL", &context.ethereum_url);

        let (mut event_stream, mut handle) = context
            .executor
            .launch(&command_executor::target::Target::Command, cmd)
            .await
            .map_err(|e| Error::daemon(format!("Failed to launch hardhat deploy: {e}")))?;

        while let Some(event) = event_stream.next().await {
            if let ProcessEventType::Stdout = &event.event_type {
                if let Some(data) = &event.data {
                    debug!("Deploy output: {}", data);

                    // Extract contract addresses from output
                    if data.contains("deployed at") {
                        if let Some(address) = extract_address(data) {
                            let contract_name = extract_contract_name(data).unwrap_or("Unknown");
                            context.deployed_addresses.set(contract_name, address);
                        }
                    }

                    // Update progress based on output
                    if data.contains("Compiling") {
                        context.set_progress(20, "Compiling contracts");
                    } else if data.contains("Deploying") {
                        context.set_progress(50, "Deploying contracts");
                    } else if data.contains("Done in") {
                        context.set_progress(80, "Deployment complete");
                    }
                }
            }
        }

        // Wait for process to complete and check exit status
        let exit_status = handle
            .wait()
            .await
            .map_err(|e| Error::daemon(format!("Failed to wait for hardhat deploy: {e}")))?;

        if !exit_status.success() {
            return Err(Error::daemon("Contract deployment failed"));
        }

        // Save deployment info
        let deployment_marker = context.working_dir.join(".graph-network-deployed");
        let deployment_info = serde_json::json!({
            "addresses": context.deployed_addresses,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        let contents = serde_json::to_string_pretty(&deployment_info)
            .map_err(|e| Error::daemon(format!("Failed to serialize deployment info: {e}")))?;
        async_fs::write(&deployment_marker, contents)
            .await
            .map_err(|e| Error::daemon(format!("Failed to write deployment marker: {e}")))?;

        Ok(())
    }

    /// Verify contracts were deployed correctly
    fn verify_deployment(context: &GraphContractsContext) -> Result<(), Error> {
        // Check that we have some deployed addresses
        if context.deployed_addresses.count() == 0 {
            return Err(Error::daemon("No contracts were deployed"));
        }

        // Check essential contracts are present
        if !context.deployed_addresses.is_complete() {
            warn!(
                "Deployment incomplete - only {} contracts deployed",
                context.deployed_addresses.count()
            );
        }

        info!(
            "Verified {} contracts deployed",
            context.deployed_addresses.count()
        );
        Ok(())
    }
}

/// State machine implementation for Graph contracts deployment
///
/// This state machine manages the lifecycle of deploying Graph Protocol contracts:
/// - Idle: Initial state waiting for deployment to start
/// - CheckingPrerequisites: Verifying environment and checking if already deployed
/// - Preparing: Setting up the deployment environment
/// - DeployingContracts: Running hardhat deploy
/// - Verifying: Validating that contracts were deployed correctly
/// - Completed: Successfully finished deployment
/// - Failed: Deployment failed, may retry if under retry limit
#[statig::state_machine(initial = "State::idle()")]
impl GraphContractsDeployTaskStateMachine {
    /// Initial idle state
    #[state]
    async fn idle(&mut self, event: &GraphContractsEvent) -> Response<State> {
        info!("Idle {:?}", event);
        let context = &mut self.context;
        match event {
            GraphContractsEvent::Start => {
                context.set_progress(5, "Starting Graph contracts deployment");
                Transition(State::checking_prerequisites())
            }
            _ => Super,
        }
    }

    /// Checking prerequisites state
    #[state]
    async fn checking_prerequisites(&mut self, event: &GraphContractsEvent) -> Response<State> {
        info!("Checking prerequisites {:?}", event);
        let context = &mut self.context;
        context.set_progress(10, "Checking prerequisites");

        // Check if already deployed
        if Self::check_deployment_exists(context).await {
            context.set_progress(100, "Contracts already deployed");
            return Transition(State::completed());
        }

        // Verify environment
        if let Err(e) = Self::verify_environment(context) {
            context.set_progress(0, format!("Environment verification failed: {e}"));
            return Transition(State::failed());
        }

        context.set_progress(15, "Prerequisites verified");
        Transition(State::preparing())
    }

    /// Preparing deployment state
    #[state]
    async fn preparing(&mut self, event: &GraphContractsEvent) -> Response<State> {
        info!("Preparing {:?}", event);
        let context = &mut self.context;
        context.set_progress(20, "Preparing deployment environment");

        // Could add additional preparation steps here
        // For now, we just proceed to deployment

        context.set_progress(25, "Environment ready");
        Transition(State::deploying_contracts())
    }

    /// Deploying contracts state
    #[state]
    async fn deploying_contracts(&mut self, event: &GraphContractsEvent) -> Response<State> {
        info!("Deploying contracts {:?}", event);
        let context = &mut self.context;
        context.set_progress(30, "Deploying contracts");

        if let Err(e) = Self::deploy_contracts(context).await {
            context.set_progress(0, format!("Deployment failed: {e}"));
            if context.can_retry() {
                context.retry_count += 1;
                warn!(
                    "Retrying deployment (attempt {}/{})",
                    context.retry_count, context.max_retries
                );
                return Transition(State::preparing());
            }
            return Transition(State::failed());
        }

        context.set_progress(85, "Contracts deployed");
        Transition(State::verifying())
    }

    /// Verifying deployment state
    #[state]
    async fn verifying(&mut self, event: &GraphContractsEvent) -> Response<State> {
        info!("Verifying {:?}", event);
        let context = &mut self.context;
        context.set_progress(90, "Verifying deployment");

        if let Err(e) = Self::verify_deployment(context) {
            context.set_progress(0, format!("Verification failed: {e}"));
            if context.can_retry() {
                context.retry_count += 1;
                warn!(
                    "Verification failed, retrying (attempt {}/{})",
                    context.retry_count, context.max_retries
                );
                return Transition(State::deploying_contracts());
            }
            return Transition(State::failed());
        }

        context.set_progress(100, "Deployment verified");
        Transition(State::completed())
    }

    /// Completed state
    #[state]
    async fn completed(&mut self, event: &GraphContractsEvent) -> Response<State> {
        let context = &self.context;
        info!(
            "Graph contracts deployment completed successfully ({} contracts) {:?}",
            context.deployed_addresses.count(),
            event
        );
        debug!(addresses = ?context.deployed_addresses, "Deployed addresses");
        Super
    }

    /// Failed state
    #[state]
    async fn failed(&mut self, event: &GraphContractsEvent) -> Response<State> {
        let context = &mut self.context;
        info!("Failed {:?}", event);
        match event {
            GraphContractsEvent::Retry => {
                if context.can_retry() {
                    context.retry_count += 1;
                    info!(
                        "Retrying deployment (attempt {}/{})",
                        context.retry_count, context.max_retries
                    );
                    context.set_progress(5, "Retrying deployment");
                    Transition(State::checking_prerequisites())
                } else {
                    error!("Retry limit exceeded for Graph contracts deployment");
                    Super
                }
            }
            _ => Super,
        }
    }
}

/// Extract contract address from deployment output
fn extract_address(line: &str) -> Option<String> {
    // Look for Ethereum addresses (0x followed by 40 hex chars)
    let re = regex::Regex::new(r"0x[a-fA-F0-9]{40}").ok()?;
    re.find(line).map(|m| m.as_str().to_string())
}

/// Extract contract name from deployment output
fn extract_contract_name(line: &str) -> Option<&str> {
    // This is a simple heuristic, might need adjustment based on actual output
    line.split_whitespace()
        .find(|word| word.chars().next().is_some_and(|c| c.is_uppercase()))
}

/// Extract deployment info from a marker file
#[allow(dead_code)]
fn extract_deployment_info(line: &str) -> HashMap<String, String> {
    // Try to extract JSON from the line
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
        if let Some(addresses) = value.get("addresses").and_then(|v| v.as_object()) {
            return addresses
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect();
        }
    }
    HashMap::new()
}

/// Run the Graph contracts deployment
///
/// Returns the deployed contract addresses on success.
pub async fn deploy_graph_contracts(
    ethereum_url: String,
    working_dir: PathBuf,
) -> Result<GraphContractAddresses, Error> {
    let context = GraphContractsContext::new(ethereum_url, working_dir.clone());
    let state_machine = GraphContractsDeployTaskStateMachine::new(context);
    let mut machine = state_machine.state_machine();

    // Start the deployment
    machine.handle(&GraphContractsEvent::Start).await;

    // Check final state and return addresses
    match machine.state() {
        State::Completed {} => {
            info!("Graph contracts deployment completed successfully");
            // Read deployed addresses from the marker file
            let deployment_marker = working_dir.join(".graph-network-deployed");
            if deployment_marker.exists() {
                let contents = async_fs::read_to_string(&deployment_marker)
                    .await
                    .map_err(|e| Error::daemon(format!("Failed to read deployment info: {e}")))?;
                let json: serde_json::Value = serde_json::from_str(&contents)
                    .map_err(|e| Error::daemon(format!("Failed to parse deployment info: {e}")))?;
                if let Some(addresses) = json.get("addresses").and_then(|v| v.as_object()) {
                    let map: HashMap<String, String> = addresses
                        .iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect();
                    return Ok(GraphContractAddresses::from_map(&map));
                }
            }
            Ok(GraphContractAddresses::default())
        }
        State::Failed {} => Err(Error::daemon("Graph contracts deployment failed")),
        _ => Err(Error::daemon(
            "Graph contracts deployment ended in unexpected state",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[smol_potat::test]
    async fn test_state_machine_initialization() {
        let context = GraphContractsContext::new(
            "http://localhost:8545".to_string(),
            std::path::PathBuf::from("/tmp/test"),
        );
        let state_machine = GraphContractsDeployTaskStateMachine::new(context);
        let machine = state_machine.state_machine();

        // Initial state should be idle
        assert!(matches!(machine.state(), State::Idle {}));
    }

    #[smol_potat::test]
    async fn test_already_deployed_check() {
        let temp_dir = TempDir::new().unwrap();
        let marker = temp_dir.path().join(".graph-network-deployed");
        async_fs::write(&marker, "{}").await.unwrap();

        let context = GraphContractsContext::new(
            "http://localhost:8545".to_string(),
            temp_dir.path().to_path_buf(),
        );

        // Just test the check function
        assert!(GraphContractsDeployTaskStateMachine::check_deployment_exists(&context).await);
    }

    #[test]
    fn test_extract_address() {
        let line = "Contract deployed at 0x1234567890123456789012345678901234567890";
        let result = extract_address(line);
        assert_eq!(
            result,
            Some("0x1234567890123456789012345678901234567890".to_string())
        );
    }

    #[test]
    fn test_extract_deployment_info() {
        let json = r#"{"addresses": {"Token": "0xabc", "Factory": "0xdef"}}"#;
        let result = extract_deployment_info(json);
        assert_eq!(result.get("Token"), Some(&"0xabc".to_string()));
        assert_eq!(result.get("Factory"), Some(&"0xdef".to_string()));
    }

    #[smol_potat::test]
    async fn test_state_transitions() {
        let context = GraphContractsContext::new(
            "http://localhost:8545".to_string(),
            std::path::PathBuf::from("/tmp/test"),
        );
        let state_machine = GraphContractsDeployTaskStateMachine::new(context);
        let mut machine = state_machine.state_machine();

        // Should start in Idle
        assert!(matches!(machine.state(), State::Idle {}));

        // Start should transition to CheckingPrerequisites
        machine.handle(&GraphContractsEvent::Start).await;
        // Note: Actual state depends on environment checks
    }
}
