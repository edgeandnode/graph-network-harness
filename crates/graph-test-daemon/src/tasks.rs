//! Graph Protocol deployment tasks
//!
//! This module defines deployment tasks for Graph Protocol components that
//! perform one-time setup operations like contract deployment.

use async_channel::Receiver;
use async_trait::async_trait;
use command_executor::{
    Command, Executor, ProcessEvent, ProcessEventType, backends::LocalLauncher, target::Target,
};
use futures::StreamExt;
use harness_core::prelude::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

/// Graph contracts deployment task
pub struct GraphContractsTask {
    /// Ethereum RPC URL
    ethereum_url: String,
    /// Working directory for contracts
    working_dir: String,
    /// Command executor
    executor: Executor<LocalLauncher>,
}

impl GraphContractsTask {
    /// Create a new Graph contracts deployment task
    pub fn new(ethereum_url: String, working_dir: String) -> Self {
        Self {
            ethereum_url,
            working_dir,
            executor: Executor::new("graph-contracts-deploy".to_string(), LocalLauncher),
        }
    }

    /// Extract contract name from compiler output
    fn extract_contract_name(line: &str) -> String {
        // Example: "Compiling contracts/Controller.sol"
        if let Some(start) = line.find("contracts/") {
            if let Some(end) = line[start..].find(".sol") {
                return line[start + 10..start + end].to_string();
            }
        }
        line.to_string()
    }

    /// Extract deployment info from hardhat output
    fn extract_deployment_info(line: &str) -> Option<(String, String)> {
        // Example: "Controller deployed at 0x5FbDB2315678afecb367f032d93F642f64180aa3"
        if line.contains("deployed at") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let name = parts[0].to_string();
                let address = parts[3].to_string();
                return Some((name, address));
            }
        }
        None
    }
}

/// Actions for Graph contracts deployment
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum GraphContractsAction {
    /// Deploy all Graph Protocol contracts
    DeployAll,
    /// Deploy a specific contract
    DeployContract { name: String },
    /// Verify deployment addresses
    VerifyDeployment,
}

/// Events from Graph contracts deployment
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event")]
pub enum GraphContractsEvent {
    /// Deployment process started
    DeploymentStarted { total_contracts: usize },
    /// Compiling a contract
    ContractCompiling { name: String },
    /// Contract deployed successfully
    ContractDeployed { name: String, address: String },
    /// Deployment progress update
    DeploymentProgress { completed: usize, total: usize },
    /// All contracts deployed
    DeploymentCompleted { addresses: HashMap<String, String> },
    /// Verification result
    VerificationResult { success: bool, message: String },
    /// Error occurred
    Error { message: String },
}

#[async_trait]
impl DeploymentTask for GraphContractsTask {
    type Action = GraphContractsAction;
    type Event = GraphContractsEvent;

    fn task_type() -> &'static str {
        "graph-contracts-deployment"
    }

    fn name(&self) -> &str {
        "graph-contracts"
    }

    fn description(&self) -> &str {
        "Deploy Graph Protocol smart contracts"
    }

    async fn is_completed(&self) -> Result<bool> {
        // Check if contracts.json exists and has addresses
        let contracts_file = std::path::Path::new(&self.working_dir).join("contracts.json");

        if contracts_file.exists() {
            // In a real implementation, parse the file and check for contract addresses
            info!("Checking if Graph contracts are already deployed");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn execute(&self, action: Self::Action) -> Result<Receiver<Self::Event>> {
        let (tx, rx) = async_channel::unbounded();

        match action {
            GraphContractsAction::DeployAll => {
                info!("Starting Graph Protocol contract deployment");

                // Send start event
                let _ = tx
                    .send(GraphContractsEvent::DeploymentStarted {
                        total_contracts: 12, // Approximate number of Graph contracts
                    })
                    .await;

                // Build hardhat command
                let mut cmd = Command::new("npx");
                cmd.args(["hardhat", "deploy", "--network", "localhost"])
                    .current_dir(&self.working_dir)
                    .env("ETHEREUM_URL", &self.ethereum_url);

                // Launch the command and get event stream
                let (mut event_stream, _handle) = self
                    .executor
                    .launch(&Target::Command, cmd)
                    .await
                    .map_err(|e| Error::daemon(format!("Failed to launch hardhat: {}", e)))?;

                // Spawn task to translate ProcessEvents to GraphContractsEvents
                let tx_clone = tx.clone();
                let mut deployed_contracts = HashMap::new();
                let mut completed = 0;

                smol::spawn(async move {
                    while let Some(event) = event_stream.next().await {
                        if let Some(translated) =
                            process_hardhat_event(&event, &mut deployed_contracts, &mut completed)
                        {
                            let _ = tx_clone.send(translated).await;
                        }
                    }

                    // Send completion event
                    let _ = tx_clone
                        .send(GraphContractsEvent::DeploymentCompleted {
                            addresses: deployed_contracts,
                        })
                        .await;
                })
                .detach();
            }

            GraphContractsAction::DeployContract { name } => {
                info!("Deploying specific contract: {}", name);

                // For specific contract deployment, would use different hardhat task
                let _ = tx
                    .send(GraphContractsEvent::Error {
                        message: "Single contract deployment not yet implemented".to_string(),
                    })
                    .await;
            }

            GraphContractsAction::VerifyDeployment => {
                info!("Verifying Graph contracts deployment");

                // Check contracts.json file
                let contracts_file = std::path::Path::new(&self.working_dir).join("contracts.json");

                if contracts_file.exists() {
                    let _ = tx
                        .send(GraphContractsEvent::VerificationResult {
                            success: true,
                            message: "Contracts deployed successfully".to_string(),
                        })
                        .await;
                } else {
                    let _ = tx
                        .send(GraphContractsEvent::VerificationResult {
                            success: false,
                            message: "No contracts.json found".to_string(),
                        })
                        .await;
                }
            }
        }

        Ok(rx)
    }
}

/// Process hardhat output events into GraphContractsEvent
fn process_hardhat_event(
    event: &ProcessEvent,
    deployed_contracts: &mut HashMap<String, String>,
    completed: &mut usize,
) -> Option<GraphContractsEvent> {
    match &event.event_type {
        ProcessEventType::Stdout => {
            if let Some(data) = &event.data {
                debug!("Hardhat output: {}", data);

                // Check for compilation
                if data.contains("Compiling") && data.contains(".sol") {
                    let contract_name = GraphContractsTask::extract_contract_name(data);
                    return Some(GraphContractsEvent::ContractCompiling {
                        name: contract_name,
                    });
                }

                // Check for deployment
                if let Some((name, address)) = GraphContractsTask::extract_deployment_info(data) {
                    deployed_contracts.insert(name.clone(), address.clone());
                    *completed += 1;

                    return Some(GraphContractsEvent::ContractDeployed { name, address });
                }

                // Check for progress indicators
                if data.contains("deploying") || data.contains("Deploying") {
                    return Some(GraphContractsEvent::DeploymentProgress {
                        completed: *completed,
                        total: 12, // Approximate
                    });
                }
            }
        }
        ProcessEventType::Stderr => {
            if let Some(data) = &event.data {
                if data.contains("Error") || data.contains("error") {
                    return Some(GraphContractsEvent::Error {
                        message: data.clone(),
                    });
                }
            }
        }
        ProcessEventType::Exited { code, .. } => {
            if let Some(code) = code {
                if *code != 0 {
                    return Some(GraphContractsEvent::Error {
                        message: format!("Deployment failed with exit code {}", code),
                    });
                }
            }
        }
        _ => {}
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use command_executor::{ProcessEvent, ProcessEventType};

    #[test]
    fn test_graph_contracts_task_creation() {
        let task = GraphContractsTask::new(
            "http://localhost:8545".to_string(),
            "./contracts".to_string(),
        );

        assert_eq!(task.name(), "graph-contracts");
        assert_eq!(task.description(), "Deploy Graph Protocol smart contracts");
        assert_eq!(
            GraphContractsTask::task_type(),
            "graph-contracts-deployment"
        );
    }

    #[test]
    fn test_process_event_compilation() {
        let mut deployed = HashMap::new();
        let mut completed = 0;

        let event = ProcessEvent {
            timestamp: chrono::Utc::now(),
            event_type: ProcessEventType::Stdout,
            data: Some("Compiling contracts/Controller.sol".to_string()),
        };

        let result = process_hardhat_event(&event, &mut deployed, &mut completed);

        assert!(result.is_some());
        if let Some(GraphContractsEvent::ContractCompiling { name }) = result {
            assert_eq!(name, "Controller");
        } else {
            panic!("Expected ContractCompiling event");
        }
    }

    #[test]
    fn test_process_event_deployment() {
        let mut deployed = HashMap::new();
        let mut completed = 0;

        let event = ProcessEvent {
            timestamp: chrono::Utc::now(),
            event_type: ProcessEventType::Stdout,
            data: Some(
                "Controller deployed at 0x5FbDB2315678afecb367f032d93F642f64180aa3".to_string(),
            ),
        };

        let result = process_hardhat_event(&event, &mut deployed, &mut completed);

        assert!(result.is_some());
        if let Some(GraphContractsEvent::ContractDeployed { name, address }) = result {
            assert_eq!(name, "Controller");
            assert_eq!(address, "0x5FbDB2315678afecb367f032d93F642f64180aa3");
            assert_eq!(completed, 1);
            assert_eq!(
                deployed.get("Controller").unwrap(),
                "0x5FbDB2315678afecb367f032d93F642f64180aa3"
            );
        } else {
            panic!("Expected ContractDeployed event");
        }
    }

    #[test]
    fn test_process_event_error() {
        let mut deployed = HashMap::new();
        let mut completed = 0;

        let event = ProcessEvent {
            timestamp: chrono::Utc::now(),
            event_type: ProcessEventType::Stderr,
            data: Some("Error: Failed to compile contracts".to_string()),
        };

        let result = process_hardhat_event(&event, &mut deployed, &mut completed);

        assert!(result.is_some());
        if let Some(GraphContractsEvent::Error { message }) = result {
            assert_eq!(message, "Error: Failed to compile contracts");
        } else {
            panic!("Expected Error event");
        }
    }

    #[test]
    fn test_process_event_exit_failure() {
        let mut deployed = HashMap::new();
        let mut completed = 0;

        let event = ProcessEvent {
            timestamp: chrono::Utc::now(),
            event_type: ProcessEventType::Exited {
                code: Some(1),
                signal: None,
            },
            data: None,
        };

        let result = process_hardhat_event(&event, &mut deployed, &mut completed);

        assert!(result.is_some());
        if let Some(GraphContractsEvent::Error { message }) = result {
            assert_eq!(message, "Deployment failed with exit code 1");
        } else {
            panic!("Expected Error event");
        }
    }

    #[test]
    fn test_process_event_progress() {
        let mut deployed = HashMap::new();
        let mut completed = 0;

        let event = ProcessEvent {
            timestamp: chrono::Utc::now(),
            event_type: ProcessEventType::Stdout,
            data: Some("Deploying Controller...".to_string()),
        };

        let result = process_hardhat_event(&event, &mut deployed, &mut completed);

        assert!(result.is_some());
        if let Some(GraphContractsEvent::DeploymentProgress {
            completed: c,
            total,
        }) = result
        {
            assert_eq!(c, 0);
            assert_eq!(total, 12);
        } else {
            panic!("Expected DeploymentProgress event");
        }
    }

    #[test]
    fn test_process_event_ignored() {
        let mut deployed = HashMap::new();
        let mut completed = 0;

        // Test irrelevant stdout
        let event = ProcessEvent {
            timestamp: chrono::Utc::now(),
            event_type: ProcessEventType::Stdout,
            data: Some("Some unrelated output".to_string()),
        };

        let result = process_hardhat_event(&event, &mut deployed, &mut completed);
        assert!(result.is_none());

        // Test other event types
        let event = ProcessEvent {
            timestamp: chrono::Utc::now(),
            event_type: ProcessEventType::Started { pid: 12345 },
            data: None,
        };

        let result = process_hardhat_event(&event, &mut deployed, &mut completed);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_contract_name() {
        assert_eq!(
            GraphContractsTask::extract_contract_name("Compiling contracts/Controller.sol"),
            "Controller"
        );

        assert_eq!(
            GraphContractsTask::extract_contract_name("Compiling contracts/GNS.sol"),
            "GNS"
        );

        // Test edge case - no contracts/ prefix
        assert_eq!(
            GraphContractsTask::extract_contract_name("Compiling something else"),
            "Compiling something else"
        );
    }

    #[test]
    fn test_extract_deployment_info() {
        let result = GraphContractsTask::extract_deployment_info(
            "Controller deployed at 0x5FbDB2315678afecb367f032d93F642f64180aa3",
        );
        assert_eq!(
            result,
            Some((
                "Controller".to_string(),
                "0x5FbDB2315678afecb367f032d93F642f64180aa3".to_string()
            ))
        );

        // Test with different contract
        let result = GraphContractsTask::extract_deployment_info(
            "GNS deployed at 0x1234567890123456789012345678901234567890",
        );
        assert_eq!(
            result,
            Some((
                "GNS".to_string(),
                "0x1234567890123456789012345678901234567890".to_string()
            ))
        );

        // Test no match
        let result = GraphContractsTask::extract_deployment_info("Some other output");
        assert!(result.is_none());
    }

    #[test]
    fn test_multiple_deployments() {
        let mut deployed = HashMap::new();
        let mut completed = 0;

        // Simulate multiple contract deployments
        let events = vec![
            (
                "Controller deployed at 0x1111111111111111111111111111111111111111",
                "Controller",
                "0x1111111111111111111111111111111111111111",
            ),
            (
                "GNS deployed at 0x2222222222222222222222222222222222222222",
                "GNS",
                "0x2222222222222222222222222222222222222222",
            ),
            (
                "Staking deployed at 0x3333333333333333333333333333333333333333",
                "Staking",
                "0x3333333333333333333333333333333333333333",
            ),
        ];

        for (line, expected_name, expected_addr) in events {
            let event = ProcessEvent {
                timestamp: chrono::Utc::now(),
                event_type: ProcessEventType::Stdout,
                data: Some(line.to_string()),
            };

            let result = process_hardhat_event(&event, &mut deployed, &mut completed);

            if let Some(GraphContractsEvent::ContractDeployed { name, address }) = result {
                assert_eq!(name, expected_name);
                assert_eq!(address, expected_addr);
            } else {
                panic!("Expected ContractDeployed event for {}", expected_name);
            }
        }

        assert_eq!(completed, 3);
        assert_eq!(deployed.len(), 3);
        assert_eq!(
            deployed.get("Controller").unwrap(),
            "0x1111111111111111111111111111111111111111"
        );
        assert_eq!(
            deployed.get("GNS").unwrap(),
            "0x2222222222222222222222222222222222222222"
        );
        assert_eq!(
            deployed.get("Staking").unwrap(),
            "0x3333333333333333333333333333333333333333"
        );
    }
}

/// TAP contracts deployment task
pub struct TapContractsTask {
    /// Ethereum RPC URL
    ethereum_url: String,
    /// Working directory for contracts
    working_dir: String,
    /// Command executor
    executor: Executor<LocalLauncher>,
}

impl TapContractsTask {
    /// Create a new TAP contracts deployment task
    pub fn new(ethereum_url: String, working_dir: String) -> Self {
        Self {
            ethereum_url,
            working_dir,
            executor: Executor::new("tap-contracts-deploy".to_string(), LocalLauncher),
        }
    }
}

/// Actions for TAP contracts deployment
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum TapContractsAction {
    /// Deploy all TAP contracts
    DeployAll,
    /// Verify deployment
    VerifyDeployment,
}

/// Events from TAP contracts deployment
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event")]
pub enum TapContractsEvent {
    /// Deployment started
    DeploymentStarted,
    /// Contract deployed
    ContractDeployed { name: String, address: String },
    /// Deployment completed
    DeploymentCompleted { addresses: HashMap<String, String> },
    /// Error occurred
    Error { message: String },
}

#[async_trait]
impl DeploymentTask for TapContractsTask {
    type Action = TapContractsAction;
    type Event = TapContractsEvent;

    fn task_type() -> &'static str {
        "tap-contracts-deployment"
    }

    fn name(&self) -> &str {
        "tap-contracts"
    }

    fn description(&self) -> &str {
        "Deploy TAP (Timeline Aggregation Protocol) contracts"
    }

    async fn is_completed(&self) -> Result<bool> {
        // Similar check for TAP contracts
        Ok(false)
    }

    async fn execute(&self, action: Self::Action) -> Result<Receiver<Self::Event>> {
        let (tx, rx) = async_channel::unbounded();

        match action {
            TapContractsAction::DeployAll => {
                info!("Starting TAP contracts deployment");

                let _ = tx.send(TapContractsEvent::DeploymentStarted).await;

                // Similar implementation to GraphContractsTask
                // but for TAP-specific contracts
            }

            TapContractsAction::VerifyDeployment => {
                info!("Verifying TAP contracts deployment");
                // Verification logic
            }
        }

        Ok(rx)
    }
}
