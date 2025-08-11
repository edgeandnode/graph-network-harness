//! TAP Protocol contracts deployment using statig state machine
//!
//! This module implements a robust state machine for deploying TAP (Timeline Aggregation Protocol)
//! contracts with proper verification and error recovery using the statig crate.

use command_executor::{
    Command, Executor, ProcessEventType, ProcessHandle, backends::LocalLauncher,
};
use futures::StreamExt;
use harness_core::{Error, Result};
use statig::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, error, info, warn};

/// States for the TAP contracts deployment state machine
#[derive(Debug, Clone, PartialEq)]
pub enum TapContractsDeployTaskState {
    /// Initial state - not started
    Idle,
    /// Checking if deployment is needed
    CheckingPrerequisites,
    /// Waiting for Graph contracts to be deployed
    WaitingForGraphContracts,
    /// Preparing environment for deployment
    Preparing,
    /// Deploying TAP contracts
    DeployingContracts,
    /// Deploying TAP subgraph
    DeployingSubgraph,
    /// Verifying deployment
    Verifying,
    /// Successfully completed
    Completed,
    /// Failed with error
    Failed,
}

/// Events that trigger state transitions
#[derive(Debug, Clone)]
pub enum TapContractsEvent {
    /// Start the deployment
    Start,
    /// Graph contracts are ready
    GraphContractsReady,
    /// Prerequisites checked - ready to proceed
    PrerequisitesReady,
    /// Already deployed - skip to completed
    AlreadyDeployed,
    /// Environment prepared
    EnvironmentReady,
    /// Contracts deployed successfully
    ContractsDeployed,
    /// Subgraph deployed successfully
    SubgraphDeployed,
    /// Verification passed
    VerificationPassed,
    /// An error occurred
    Error(String),
    /// Retry the deployment
    Retry,
}

/// Shared context for the state machine
pub struct TapContractsContext {
    /// Ethereum RPC URL
    pub ethereum_url: String,
    /// Working directory for contracts
    pub working_dir: PathBuf,
    /// Command executor
    pub executor: Executor<LocalLauncher>,
    /// Deployed TAP contract addresses
    pub deployed_addresses: HashMap<String, String>,
    /// Graph Protocol contract addresses (needed as dependencies)
    pub graph_addresses: HashMap<String, String>,
    /// Subgraph deployment ID
    pub subgraph_deployment_id: Option<String>,
    /// Current progress (0-100)
    pub progress: u8,
    /// Status message
    pub status_message: String,
    /// Retry count
    pub retry_count: u32,
    /// Maximum retries
    pub max_retries: u32,
}

impl TapContractsContext {
    /// Create a new context
    pub fn new(ethereum_url: String, working_dir: PathBuf) -> Self {
        Self {
            ethereum_url,
            working_dir,
            executor: Executor::new("tap-contracts-deploy".to_string(), LocalLauncher),
            deployed_addresses: HashMap::new(),
            graph_addresses: HashMap::new(),
            subgraph_deployment_id: None,
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
            "TAP contracts deployment progress"
        );
    }

    /// Check if retries available
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }
}

/// State machine for TAP contracts deployment
pub struct TapContractsDeployTaskStateMachine {
    context: TapContractsContext,
}

impl TapContractsDeployTaskStateMachine {
    /// Create a new state machine
    pub fn new(context: TapContractsContext) -> Self {
        Self { context }
    }

    /// Check if TAP subgraph already exists
    async fn check_subgraph_exists(context: &TapContractsContext) -> bool {
        let deployment_marker = context.working_dir.join(".tap-deployed");
        deployment_marker.exists()
    }

    /// Check if Graph contracts are deployed
    async fn check_graph_contracts(context: &mut TapContractsContext) -> Result<bool> {
        let graph_addresses_file = context
            .working_dir
            .parent()
            .ok_or_else(|| Error::daemon("Working directory has no parent"))?
            .join("graph-contracts/deployed-addresses.json");

        if !graph_addresses_file.exists() {
            info!("Graph contracts not yet deployed");
            return Ok(false);
        }

        let contents = async_fs::read_to_string(&graph_addresses_file)
            .await
            .map_err(|e| Error::daemon(format!("Failed to read Graph addresses: {e}")))?;

        let json: serde_json::Value = serde_json::from_str(&contents)
            .map_err(|e| Error::daemon(format!("Failed to parse Graph addresses: {e}")))?;

        if let Some(chain_data) = json.get("1337") {
            if let Some(contracts) = chain_data.as_object() {
                for (name, data) in contracts {
                    if let Some(address) = data.as_str() {
                        context
                            .graph_addresses
                            .insert(name.clone(), address.to_string());
                    }
                }
            }
        }

        if context.graph_addresses.is_empty() {
            info!("No Graph contract addresses found");
            return Ok(false);
        }

        info!(
            "Found {} Graph contract addresses",
            context.graph_addresses.len()
        );
        Ok(true)
    }

    /// Verify working directory and required files exist
    fn verify_environment(context: &TapContractsContext) -> Result<()> {
        if !context.working_dir.exists() {
            return Err(Error::daemon(format!(
                "Working directory does not exist: {}",
                context.working_dir.display()
            )));
        }

        // Check for forge configuration
        let foundry_toml = context.working_dir.join("foundry.toml");
        if !foundry_toml.exists() {
            return Err(Error::daemon("foundry.toml not found in working directory"));
        }

        Ok(())
    }

    /// Deploy TAP contracts using forge
    async fn deploy_contracts(context: &mut TapContractsContext) -> Result<()> {
        info!("Deploying TAP contracts");

        // TAP contracts typically include:
        // 1. TAP Verifier
        // 2. TAP Collector
        // 3. Escrow contracts

        let mut cmd = Command::new("forge");
        cmd.args([
            "script",
            "script/Deploy.s.sol",
            "--rpc-url",
            &context.ethereum_url,
            "--broadcast",
        ])
        .current_dir(&context.working_dir)
        .env("ETHEREUM_URL", &context.ethereum_url);

        // Add Graph contract addresses as environment variables
        for (name, address) in &context.graph_addresses {
            cmd.env(format!("GRAPH_{}", name.to_uppercase()), address);
        }

        let (mut event_stream, mut handle) = context
            .executor
            .launch(&command_executor::target::Target::Command, cmd)
            .await
            .map_err(|e| Error::daemon(format!("Failed to launch forge: {e}")))?;

        let mut completed_count = 0;
        let total_contracts = 3; // TAP typically has 3 main contracts

        while let Some(event) = event_stream.next().await {
            if let ProcessEventType::Stdout = &event.event_type {
                if let Some(data) = &event.data {
                    debug!("Forge output: {}", data);

                    // Forge output format: "Contract deployed: 0x..."
                    if data.contains("Contract deployed:") || data.contains("deployed at") {
                        if let Some(address) = extract_address(data) {
                            let contract_name = if data.contains("Verifier") {
                                "TAPVerifier"
                            } else if data.contains("Collector") {
                                "TAPCollector"
                            } else {
                                "Escrow"
                            };

                            context
                                .deployed_addresses
                                .insert(contract_name.to_string(), address.clone());
                            completed_count += 1;

                            let progress = 20 + (completed_count * 40 / total_contracts) as u8;
                            context.set_progress(
                                progress,
                                format!("Deployed {completed_count} contracts"),
                            );
                        }
                    }
                }
            }
        }

        // Wait for process to complete and check exit status
        let exit_status = handle
            .wait()
            .await
            .map_err(|e| Error::daemon(format!("Failed to wait for forge: {e}")))?;

        if !exit_status.success() {
            return Err(Error::daemon("TAP contract deployment failed"));
        }

        info!(
            "Deployed {} TAP contracts",
            context.deployed_addresses.len()
        );

        // Save deployed addresses
        let addresses_file = context.working_dir.join("tap-addresses.json");
        let json = serde_json::json!({
            "1337": context.deployed_addresses
        });
        let contents = serde_json::to_string_pretty(&json)
            .map_err(|e| Error::daemon(format!("Failed to serialize addresses: {e}")))?;
        async_fs::write(&addresses_file, contents)
            .await
            .map_err(|e| Error::daemon(format!("Failed to write addresses file: {e}")))?;

        Ok(())
    }

    /// Deploy the TAP subgraph
    async fn deploy_subgraph(context: &mut TapContractsContext) -> Result<()> {
        info!("Deploying TAP subgraph");

        // Create subgraph
        let mut cmd = Command::new("npx");
        cmd.args([
            "graph",
            "create",
            "tap-subgraph",
            "--node",
            "http://localhost:8020",
        ])
        .current_dir(&context.working_dir);

        let _ = context
            .executor
            .launch(&command_executor::target::Target::Command, cmd)
            .await
            .map_err(|e| Error::daemon(format!("Failed to create TAP subgraph: {e}")))?;

        context.set_progress(70, "Created TAP subgraph, deploying...");

        // Deploy subgraph
        let mut cmd = Command::new("npx");
        cmd.args([
            "graph",
            "deploy",
            "tap-subgraph",
            "--node",
            "http://localhost:8020",
            "--ipfs",
            "http://localhost:5001",
            "--version-label",
            "v0.0.1",
        ])
        .current_dir(&context.working_dir);

        let (mut event_stream, mut handle) = context
            .executor
            .launch(&command_executor::target::Target::Command, cmd)
            .await
            .map_err(|e| Error::daemon(format!("Failed to deploy TAP subgraph: {e}")))?;

        while let Some(event) = event_stream.next().await {
            if let ProcessEventType::Stdout = &event.event_type {
                if let Some(data) = &event.data {
                    if data.contains("Build completed:") {
                        if let Some(id) = extract_deployment_id(data) {
                            context.subgraph_deployment_id = Some(id.clone());
                            context.set_progress(85, "TAP subgraph built and uploaded");
                        }
                    }
                }
            }
        }

        // Wait for process to complete and check exit status
        let exit_status = handle.wait().await.map_err(|e| {
            Error::daemon(format!("Failed to wait for TAP subgraph deployment: {e}"))
        })?;

        if !exit_status.success() {
            return Err(Error::daemon("TAP subgraph deployment failed"));
        }

        if let Some(ref id) = context.subgraph_deployment_id {
            let marker = context.working_dir.join(".tap-deployed");
            async_fs::write(&marker, id.as_bytes())
                .await
                .map_err(|e| Error::daemon(format!("Failed to write deployment marker: {e}")))?;
        }

        Ok(())
    }

    /// Verify deployment succeeded
    fn verify_deployment(context: &TapContractsContext) -> Result<()> {
        // Verify we have deployed addresses
        if context.deployed_addresses.is_empty() {
            return Err(Error::daemon("No TAP contracts were deployed"));
        }

        // Verify we have the expected contracts
        let expected = ["TAPVerifier", "TAPCollector", "Escrow"];
        for name in &expected {
            if !context.deployed_addresses.contains_key(*name) {
                return Err(Error::daemon(format!("Missing TAP contract: {name}")));
            }
        }

        // Verify subgraph deployment
        if context.subgraph_deployment_id.is_none() {
            return Err(Error::daemon("TAP subgraph deployment ID not found"));
        }

        Ok(())
    }
}

/// State machine implementation for TAP contracts deployment
///
/// This state machine manages the lifecycle of deploying TAP (Timeline Aggregation Protocol) contracts:
/// - Idle: Initial state waiting for deployment to start  
/// - CheckingPrerequisites: Verifying Graph contracts are deployed (dependency)
/// - WaitingForGraphContracts: Waiting for Graph Protocol contracts to be ready
/// - Preparing: Setting up environment and verifying working directory
/// - DeployingContracts: Deploying TAP smart contracts using forge
/// - DeployingSubgraph: Deploying the TAP subgraph to Graph Node
/// - Verifying: Validating that all deployments were successful
/// - Completed: Successfully finished TAP deployment
/// - Failed: Deployment failed, may retry if under retry limit
#[statig::state_machine(initial = "State::idle()")]
impl TapContractsDeployTaskStateMachine {
    /// Initial idle state
    #[state]
    async fn idle(&mut self, event: &TapContractsEvent) -> Response<State> {
        let context = &mut self.context;
        match event {
            TapContractsEvent::Start => {
                context.set_progress(5, "Starting TAP deployment");
                Transition(State::checking_prerequisites())
            }
            _ => Super,
        }
    }

    /// Checking prerequisites state
    #[state]
    async fn checking_prerequisites(&mut self, event: &TapContractsEvent) -> Response<State> {
        let context = &mut self.context;
        context.set_progress(10, "Checking prerequisites");

        // Check if already deployed
        if Self::check_subgraph_exists(context).await {
            context.set_progress(100, "TAP already deployed");
            return Transition(State::completed());
        }

        // Check if Graph contracts are deployed
        match Self::check_graph_contracts(context).await {
            Ok(true) => {
                context.set_progress(15, "Graph contracts found");
                Transition(State::preparing())
            }
            Ok(false) => {
                context.set_progress(12, "Waiting for Graph contracts");
                Transition(State::waiting_for_graph_contracts())
            }
            Err(e) => {
                context.set_progress(0, format!("Failed to check Graph contracts: {e}"));
                Transition(State::failed())
            }
        }
    }

    /// Waiting for Graph contracts state
    #[state]
    async fn waiting_for_graph_contracts(&mut self, event: &TapContractsEvent) -> Response<State> {
        let context = &mut self.context;
        match event {
            TapContractsEvent::GraphContractsReady => {
                context.set_progress(15, "Graph contracts ready");
                Transition(State::preparing())
            }
            _ => {
                // Periodically check if Graph contracts are ready
                match Self::check_graph_contracts(context).await {
                    Ok(true) => Transition(State::preparing()),
                    _ => Super,
                }
            }
        }
    }

    /// Preparing environment state
    #[state]
    async fn preparing(&mut self, event: &TapContractsEvent) -> Response<State> {
        let context = &mut self.context;
        context.set_progress(20, "Preparing environment");

        if let Err(e) = Self::verify_environment(context) {
            context.set_progress(0, format!("Environment verification failed: {e}"));
            return Transition(State::failed());
        }

        context.set_progress(25, "Environment ready");
        Transition(State::deploying_contracts())
    }

    /// Deploying contracts state
    #[state]
    async fn deploying_contracts(&mut self, event: &TapContractsEvent) -> Response<State> {
        let context = &mut self.context;
        context.set_progress(30, "Deploying TAP contracts");

        if let Err(e) = Self::deploy_contracts(context).await {
            context.set_progress(0, format!("TAP contract deployment failed: {e}"));
            if context.can_retry() {
                context.retry_count += 1;
                warn!(
                    "Retrying TAP contract deployment (attempt {}/{})",
                    context.retry_count, context.max_retries
                );
                return Transition(State::preparing());
            }
            return Transition(State::failed());
        }

        context.set_progress(65, "TAP contracts deployed");
        Transition(State::deploying_subgraph())
    }

    /// Deploying subgraph state
    #[state]
    async fn deploying_subgraph(&mut self, event: &TapContractsEvent) -> Response<State> {
        let context = &mut self.context;
        context.set_progress(70, "Deploying TAP subgraph");

        if let Err(e) = Self::deploy_subgraph(context).await {
            context.set_progress(0, format!("TAP subgraph deployment failed: {e}"));
            if context.can_retry() {
                context.retry_count += 1;
                warn!(
                    "Retrying from beginning (attempt {}/{})",
                    context.retry_count, context.max_retries
                );
                return Transition(State::preparing());
            }
            return Transition(State::failed());
        }

        context.set_progress(90, "TAP subgraph deployed");
        Transition(State::verifying())
    }

    /// Verifying deployment state
    #[state]
    async fn verifying(&mut self, event: &TapContractsEvent) -> Response<State> {
        let context = &mut self.context;
        context.set_progress(95, "Verifying TAP deployment");

        if let Err(e) = Self::verify_deployment(context) {
            context.set_progress(0, format!("Verification failed: {e}"));
            if context.can_retry() {
                context.retry_count += 1;
                warn!(
                    "TAP verification failed, retrying (attempt {}/{})",
                    context.retry_count, context.max_retries
                );
                return Transition(State::preparing());
            }
            return Transition(State::failed());
        }

        context.set_progress(100, "TAP deployment verified");
        Transition(State::completed())
    }

    /// Completed state
    #[state]
    async fn completed(&mut self, event: &TapContractsEvent) -> Response<State> {
        info!("TAP contracts deployment completed successfully");
        Super
    }

    /// Failed state
    #[state]
    async fn failed(&mut self, event: &TapContractsEvent) -> Response<State> {
        let context = &mut self.context;
        match event {
            TapContractsEvent::Retry => {
                if context.can_retry() {
                    context.retry_count += 1;
                    info!(
                        "Retrying TAP deployment (attempt {}/{})",
                        context.retry_count, context.max_retries
                    );
                    context.set_progress(5, "Retrying TAP deployment");
                    Transition(State::checking_prerequisites())
                } else {
                    error!("TAP retry limit exceeded");
                    Super
                }
            }
            _ => Super,
        }
    }
}

/// Extract address from forge output
fn extract_address(line: &str) -> Option<String> {
    // Look for Ethereum address pattern (0x followed by 40 hex chars)
    if let Some(start) = line.find("0x") {
        let address: String = line[start..]
            .chars()
            .take(42) // 0x + 40 hex chars
            .collect();
        if address.len() == 42 && address[2..].chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(address);
        }
    }
    None
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

/// Run the TAP contracts deployment
pub async fn deploy_tap_contracts(ethereum_url: String, working_dir: PathBuf) -> Result<()> {
    let context = TapContractsContext::new(ethereum_url, working_dir);
    let state_machine = TapContractsDeployTaskStateMachine::new(context);
    let mut machine = state_machine.state_machine();

    // Start the deployment
    machine.handle(&TapContractsEvent::Start).await;

    // Check final state
    match machine.state() {
        State::Completed {} => {
            info!("TAP contracts deployment completed successfully");
            Ok(())
        }
        State::WaitingForGraphContracts {} => Err(Error::daemon(
            "TAP contracts deployment waiting for Graph contracts",
        )),
        State::Failed {} => Err(Error::daemon("TAP contracts deployment failed")),
        _ => Err(Error::daemon(
            "TAP contracts deployment ended in unexpected state",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Removed unused statig prelude import
    use tempfile::TempDir;

    #[smol_potat::test]
    async fn test_state_machine_initialization() {
        let context = TapContractsContext::new(
            "http://localhost:8545".to_string(),
            std::path::PathBuf::from("/tmp/test"),
        );
        let state_machine = TapContractsDeployTaskStateMachine::new(context);
        let machine = state_machine.state_machine();

        // Initial state should be idle
        assert!(matches!(machine.state(), State::Idle {}));
    }

    #[smol_potat::test]
    async fn test_already_deployed_check() {
        let temp_dir = TempDir::new().unwrap();
        let marker = temp_dir.path().join(".tap-deployed");
        async_fs::write(&marker, "QmTest").await.unwrap();

        let context = TapContractsContext::new(
            "http://localhost:8545".to_string(),
            temp_dir.path().to_path_buf(),
        );

        // Just test the check function, don't start the full state machine
        assert!(TapContractsDeployTaskStateMachine::check_subgraph_exists(&context).await);
    }

    #[smol_potat::test]
    async fn test_state_transitions() {
        let temp_dir = TempDir::new().unwrap();
        let context = TapContractsContext::new(
            "http://localhost:8545".to_string(),
            temp_dir.path().to_path_buf(),
        );

        let state_machine = TapContractsDeployTaskStateMachine::new(context);
        let mut machine = state_machine.state_machine();

        // Test that we can handle the Start event
        // Note: This won't actually deploy anything, just test state transitions
        machine.handle(&TapContractsEvent::Start).await;

        // After start, we should transition from idle
        // The exact state depends on prerequisites check
        assert!(!matches!(machine.state(), State::Idle {}));
    }

    #[test]
    fn test_extract_address() {
        let line = "Contract deployed: 0x5FbDB2315678afecb367f032d93F642f64180aa3";
        let result = extract_address(line);
        assert_eq!(
            result,
            Some("0x5FbDB2315678afecb367f032d93F642f64180aa3".to_string())
        );
    }

    #[test]
    fn test_extract_deployment_id() {
        let line = "Build completed: QmTAP123abc456def789";
        let result = extract_deployment_id(line);
        assert_eq!(result, Some("QmTAP123abc456def789".to_string()));
    }
}
