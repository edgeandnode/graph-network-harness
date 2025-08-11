//! Graph Protocol contracts deployment using statig state machine
//!
//! This module implements a robust state machine for deploying Graph Protocol
//! contracts with proper verification and error recovery using the statig crate.

use async_trait::async_trait;
use command_executor::{Command, Executor, ProcessEvent, ProcessEventType, ProcessHandle, backends::LocalLauncher};
use futures::StreamExt;
use harness_core::{Error, Result};
use serde::{Deserialize, Serialize};
use statig::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};

/// States for the Graph contracts deployment state machine
#[derive(Debug, Clone, PartialEq)]
pub enum GraphContractsDeployTaskState {
    /// Initial state - not started
    Idle,
    /// Checking if deployment is needed
    CheckingPrerequisites,
    /// Preparing environment for deployment
    Preparing,
    /// Deploying contracts
    DeployingContracts,
    /// Deploying subgraph
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
pub enum GraphContractsEvent {
    /// Start the deployment
    Start,
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
pub struct GraphContractsContext {
    /// Ethereum RPC URL
    pub ethereum_url: String,
    /// Working directory for contracts
    pub working_dir: PathBuf,
    /// Command executor
    pub executor: Executor<LocalLauncher>,
    /// Deployed contract addresses
    pub deployed_addresses: HashMap<String, String>,
    /// Expected contract addresses (for verification)
    pub expected_addresses: HashMap<String, String>,
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

impl GraphContractsContext {
    /// Create a new context
    pub fn new(ethereum_url: String, working_dir: PathBuf) -> Self {
        Self {
            ethereum_url,
            working_dir,
            executor: Executor::new("graph-contracts-deploy".to_string(), LocalLauncher),
            deployed_addresses: HashMap::new(),
            expected_addresses: HashMap::new(),
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

    /// Check if graph-network subgraph already exists
    async fn check_subgraph_exists(context: &GraphContractsContext) -> bool {
        let deployment_marker = context.working_dir.join(".graph-network-deployed");
        deployment_marker.exists()
    }

    /// Load expected addresses from contracts.json
    async fn load_expected_addresses(context: &mut GraphContractsContext) -> Result<()> {
        let contracts_file = context.working_dir.join("contracts.json");
        
        if contracts_file.exists() {
            let contents = async_fs::read_to_string(&contracts_file).await
                .map_err(|e| Error::daemon(format!("Failed to read contracts.json: {}", e)))?;
            
            let json: serde_json::Value = serde_json::from_str(&contents)
                .map_err(|e| Error::daemon(format!("Failed to parse contracts.json: {}", e)))?;
            
            if let Some(chain_data) = json.get("1337") {
                if let Some(contracts) = chain_data.as_object() {
                    for (name, data) in contracts {
                        if let Some(address) = data.get("address").and_then(|a| a.as_str()) {
                            context.expected_addresses.insert(name.clone(), address.to_string());
                        }
                    }
                }
            }
            
            info!("Loaded {} expected contract addresses", context.expected_addresses.len());
        }
        
        Ok(())
    }

    /// Verify working directory and required files exist
    fn verify_environment(context: &GraphContractsContext) -> Result<()> {
        if !context.working_dir.exists() {
            return Err(Error::daemon(format!(
                "Working directory does not exist: {}",
                context.working_dir.display()
            )));
        }

        let package_json = context.working_dir.join("package.json");
        if !package_json.exists() {
            return Err(Error::daemon("package.json not found in working directory"));
        }

        let hardhat_config = context.working_dir.join("hardhat.config.js").exists() 
            || context.working_dir.join("hardhat.config.ts").exists();
        
        if !hardhat_config {
            return Err(Error::daemon("hardhat.config not found"));
        }

        Ok(())
    }

    /// Deploy contracts using hardhat
    async fn deploy_contracts(context: &mut GraphContractsContext) -> Result<()> {
        info!("Deploying Graph Protocol contracts");
        
        let mut cmd = Command::new("npx");
        cmd.args(["hardhat", "deploy", "--network", "localhost"])
            .current_dir(&context.working_dir)
            .env("ETHEREUM_URL", &context.ethereum_url);
        
        let (mut event_stream, mut handle) = context.executor
            .launch(&command_executor::target::Target::Command, cmd)
            .await
            .map_err(|e| Error::daemon(format!("Failed to launch hardhat: {}", e)))?;
        
        let mut completed_count = 0;
        let total_contracts = 12;
        
        while let Some(event) = event_stream.next().await {
            if let ProcessEventType::Stdout = &event.event_type {
                if let Some(data) = &event.data {
                    debug!("Hardhat output: {}", data);
                    
                    if data.contains("deployed at") {
                        if let Some((name, address)) = extract_deployment_info(data) {
                            context.deployed_addresses.insert(name.clone(), address.clone());
                            completed_count += 1;
                            
                            let progress = 20 + (completed_count * 40 / total_contracts) as u8;
                            context.set_progress(progress, format!("Deployed {} contracts", completed_count));
                        }
                    }
                }
            }
        }
        
        // Wait for process to complete and check exit status
        let exit_status = handle.wait().await
            .map_err(|e| Error::daemon(format!("Failed to wait for hardhat: {}", e)))?;
        
        if !exit_status.success() {
            return Err(Error::daemon("Contract deployment failed"));
        }
        
        info!("Deployed {} contracts", context.deployed_addresses.len());
        
        // Save deployed addresses
        let addresses_file = context.working_dir.join("deployed-addresses.json");
        let json = serde_json::json!({
            "1337": context.deployed_addresses
        });
        let contents = serde_json::to_string_pretty(&json)
            .map_err(|e| Error::daemon(format!("Failed to serialize addresses: {}", e)))?;
        async_fs::write(&addresses_file, contents).await
            .map_err(|e| Error::daemon(format!("Failed to write addresses file: {}", e)))?;
        
        Ok(())
    }

    /// Deploy the graph-network subgraph
    async fn deploy_subgraph(context: &mut GraphContractsContext) -> Result<()> {
        info!("Deploying graph-network subgraph");
        
        // Create subgraph
        let mut cmd = Command::new("npx");
        cmd.args([
            "graph", "create", "graph-network",
            "--node", "http://localhost:8020"
        ])
        .current_dir(&context.working_dir);
        
        let _ = context.executor
            .launch(&command_executor::target::Target::Command, cmd)
            .await
            .map_err(|e| Error::daemon(format!("Failed to create subgraph: {}", e)))?;
        
        context.set_progress(70, "Created subgraph, deploying...");
        
        // Deploy subgraph
        let mut cmd = Command::new("npx");
        cmd.args([
            "graph", "deploy", "graph-network",
            "--node", "http://localhost:8020",
            "--ipfs", "http://localhost:5001",
            "--version-label", "v0.0.1"
        ])
        .current_dir(&context.working_dir);
        
        let (mut event_stream, mut handle) = context.executor
            .launch(&command_executor::target::Target::Command, cmd)
            .await
            .map_err(|e| Error::daemon(format!("Failed to deploy subgraph: {}", e)))?;
        
        while let Some(event) = event_stream.next().await {
            if let ProcessEventType::Stdout = &event.event_type {
                if let Some(data) = &event.data {
                    if data.contains("Build completed:") {
                        if let Some(id) = extract_deployment_id(data) {
                            context.subgraph_deployment_id = Some(id.clone());
                            context.set_progress(85, "Subgraph built and uploaded");
                        }
                    }
                }
            }
        }
        
        // Wait for process to complete and check exit status
        let exit_status = handle.wait().await
            .map_err(|e| Error::daemon(format!("Failed to wait for subgraph deployment: {}", e)))?;
        
        if !exit_status.success() {
            return Err(Error::daemon("Subgraph deployment failed"));
        }
        
        if let Some(ref id) = context.subgraph_deployment_id {
            let marker = context.working_dir.join(".graph-network-deployed");
            async_fs::write(&marker, id.as_bytes()).await
                .map_err(|e| Error::daemon(format!("Failed to write deployment marker: {}", e)))?;
        }
        
        Ok(())
    }

    /// Verify deployment succeeded
    fn verify_deployment(context: &GraphContractsContext) -> Result<()> {
        // Verify we have deployed addresses
        if context.deployed_addresses.is_empty() {
            return Err(Error::daemon("No contracts were deployed"));
        }

        // Verify against expected if available
        if !context.expected_addresses.is_empty() {
            for (name, expected) in &context.expected_addresses {
                if let Some(deployed) = context.deployed_addresses.get(name) {
                    if deployed != expected {
                        return Err(Error::daemon(format!(
                            "Address mismatch for {}: expected {} but got {}",
                            name, expected, deployed
                        )));
                    }
                }
            }
        }

        // Verify subgraph deployment
        if context.subgraph_deployment_id.is_none() {
            return Err(Error::daemon("Subgraph deployment ID not found"));
        }

        Ok(())
    }
}

// Implement statig state machine
#[statig::state_machine(initial = "State::idle()")]
impl GraphContractsDeployTaskStateMachine {
    /// Initial idle state
    #[state]
    async fn idle(&mut self, event: &GraphContractsEvent) -> Response<State> {
        let context = &mut self.context;
        match event {
            GraphContractsEvent::Start => {
                context.set_progress(5, "Starting deployment");
                Transition(State::checking_prerequisites())
            }
            _ => Super
        }
    }

    /// Checking prerequisites state
    #[state]
    async fn checking_prerequisites(&mut self, event: &GraphContractsEvent) -> Response<State> {
        let context = &mut self.context;
        // This state should run its logic on entry
        match event {
            GraphContractsEvent::Start => {
                context.set_progress(10, "Checking prerequisites");
                
                // Load expected addresses
                if let Err(e) = Self::load_expected_addresses(context).await {
                    context.set_progress(0, format!("Failed to load expected addresses: {}", e));
                    return Transition(State::failed());
                }
                
                // Check if already deployed
                if Self::check_subgraph_exists(context).await {
                    context.set_progress(100, "Already deployed");
                    return Transition(State::completed());
                }
                
                Transition(State::preparing())
            }
            _ => Super
        }
    }

    /// Preparing environment state
    #[state]
    async fn preparing(&mut self, event: &GraphContractsEvent) -> Response<State> {
        let context = &mut self.context;
        context.set_progress(15, "Preparing environment");
        
        if let Err(e) = Self::verify_environment(context) {
            context.set_progress(0, format!("Environment verification failed: {}", e));
            return Transition(State::failed());
        }
        
        context.set_progress(20, "Environment ready");
        Transition(State::deploying_contracts())
    }

    /// Deploying contracts state
    #[state]
    async fn deploying_contracts(&mut self, event: &GraphContractsEvent) -> Response<State> {
        let context = &mut self.context;
        context.set_progress(25, "Deploying contracts");
        
        if let Err(e) = Self::deploy_contracts(context).await {
            context.set_progress(0, format!("Contract deployment failed: {}", e));
            if context.can_retry() {
                context.retry_count += 1;
                warn!("Retrying contract deployment (attempt {}/{})", context.retry_count, context.max_retries);
                return Transition(State::preparing());
            }
            return Transition(State::failed());
        }
        
        context.set_progress(65, "Contracts deployed");
        Transition(State::deploying_subgraph())
    }

    /// Deploying subgraph state
    #[state]
    async fn deploying_subgraph(&mut self, event: &GraphContractsEvent) -> Response<State> {
        let context = &mut self.context;
        context.set_progress(70, "Deploying subgraph");
        
        if let Err(e) = Self::deploy_subgraph(context).await {
            context.set_progress(0, format!("Subgraph deployment failed: {}", e));
            if context.can_retry() {
                context.retry_count += 1;
                warn!("Retrying from beginning (attempt {}/{})", context.retry_count, context.max_retries);
                return Transition(State::preparing());
            }
            return Transition(State::failed());
        }
        
        context.set_progress(90, "Subgraph deployed");
        Transition(State::verifying())
    }

    /// Verifying deployment state
    #[state]
    async fn verifying(&mut self, event: &GraphContractsEvent) -> Response<State> {
        let context = &mut self.context;
        context.set_progress(95, "Verifying deployment");
        
        if let Err(e) = Self::verify_deployment(context) {
            context.set_progress(0, format!("Verification failed: {}", e));
            if context.can_retry() {
                context.retry_count += 1;
                warn!("Verification failed, retrying (attempt {}/{})", context.retry_count, context.max_retries);
                return Transition(State::preparing());
            }
            return Transition(State::failed());
        }
        
        context.set_progress(100, "Deployment verified");
        Transition(State::completed())
    }

    /// Completed state
    #[state]
    async fn completed(&mut self, event: &GraphContractsEvent) -> Response<State> {
        info!("Graph contracts deployment completed successfully");
        Super
    }

    /// Failed state
    #[state]
    async fn failed(&mut self, event: &GraphContractsEvent) -> Response<State> {
        let context = &mut self.context;
        match event {
            GraphContractsEvent::Retry => {
                if context.can_retry() {
                    context.retry_count += 1;
                    info!("Retrying deployment (attempt {}/{})", context.retry_count, context.max_retries);
                    context.set_progress(5, "Retrying deployment");
                    Transition(State::checking_prerequisites())
                } else {
                    error!("Retry limit exceeded");
                    Super
                }
            }
            _ => Super
        }
    }
}

/// Extract deployment info from hardhat output
fn extract_deployment_info(line: &str) -> Option<(String, String)> {
    if line.contains("deployed at") {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 {
            return Some((parts[0].to_string(), parts[3].to_string()));
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

/// Run the Graph contracts deployment
pub async fn deploy_graph_contracts(ethereum_url: String, working_dir: PathBuf) -> Result<()> {
    let context = GraphContractsContext::new(ethereum_url, working_dir);
    let state_machine = GraphContractsDeployTaskStateMachine::new(context);
    let mut machine = state_machine.state_machine();
    
    // Start the deployment
    machine.handle(&GraphContractsEvent::Start).await;
    
    // The state machine will automatically progress through states
    // We could add more event handling here if needed
    
    // Check final state
    match machine.state() {
        State::Completed {} => {
            info!("Graph contracts deployment completed successfully");
            Ok(())
        }
        State::Failed {} => {
            Err(Error::daemon("Graph contracts deployment failed"))
        }
        _ => {
            Err(Error::daemon(
                "Graph contracts deployment ended in unexpected state"
            ))
        }
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
            std::path::PathBuf::from("/tmp/test")
        );
        let machine = GraphContractsDeployTaskStateMachine::new(context);
        // State is managed internally by statig
    }

    #[smol_potat::test]
    async fn test_already_deployed_check() {
        let temp_dir = TempDir::new().unwrap();
        let marker = temp_dir.path().join(".graph-network-deployed");
        async_fs::write(&marker, "QmTest").await.unwrap();
        
        let context = GraphContractsContext::new(
            "http://localhost:8545".to_string(),
            temp_dir.path().to_path_buf()
        );
        
        assert!(GraphContractsDeployTaskStateMachine::check_subgraph_exists(&context).await);
    }

    #[test]
    fn test_extract_deployment_info() {
        let line = "Controller deployed at 0x5FbDB2315678afecb367f032d93F642f64180aa3";
        let result = extract_deployment_info(line);
        assert_eq!(
            result,
            Some(("Controller".to_string(), "0x5FbDB2315678afecb367f032d93F642f64180aa3".to_string()))
        );
    }

    #[test]
    fn test_extract_deployment_id() {
        let line = "Build completed: QmXYZ123abc456def789";
        let result = extract_deployment_id(line);
        assert_eq!(result, Some("QmXYZ123abc456def789".to_string()));
    }
}