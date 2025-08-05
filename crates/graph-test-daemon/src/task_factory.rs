//! Factory for creating deployment tasks
//!
//! This module provides a factory that creates task instances that implement
//! the DeploymentTask trait for various Graph Protocol operations.

use std::sync::Arc;

use crate::tasks::{GraphContractsTask, SubgraphDeployTask, TapContractsTask};

/// Task handle that can execute tasks
pub enum TaskHandle {
    /// Graph contracts deployment task
    GraphContracts(Arc<GraphContractsTask>),
    /// Subgraph deployment task
    SubgraphDeploy(Arc<SubgraphDeployTask>),
    /// TAP contracts deployment task
    TapContracts(Arc<TapContractsTask>),
}

impl TaskHandle {
    /// Get the task name
    pub fn name(&self) -> &str {
        match self {
            TaskHandle::GraphContracts(task) => task.name(),
            TaskHandle::SubgraphDeploy(task) => task.name(),
            TaskHandle::TapContracts(task) => task.name(),
        }
    }

    /// Check if the task is completed
    pub async fn is_completed(&self) -> harness_core::Result<bool> {
        match self {
            TaskHandle::GraphContracts(task) => task.is_completed().await,
            TaskHandle::SubgraphDeploy(task) => task.is_completed().await,
            TaskHandle::TapContracts(task) => task.is_completed().await,
        }
    }
}

/// Factory for creating deployment tasks
pub struct TaskFactory;

impl TaskFactory {
    /// Create a task instance by type
    pub fn create_task(task_type: &str) -> Option<TaskHandle> {
        match task_type {
            "graph-contracts" | "graph-contracts-deployment" => {
                // Use default values for now
                Some(TaskHandle::GraphContracts(Arc::new(
                    GraphContractsTask::new(
                        "http://localhost:8545".to_string(),
                        "./contracts".to_string(),
                    ),
                )))
            }
            "subgraph" | "subgraph-deployment" => {
                Some(TaskHandle::SubgraphDeploy(Arc::new(
                    SubgraphDeployTask::new(
                        "http://localhost:8000".to_string(),
                        "http://localhost:5001".to_string(),
                        "http://localhost:8545".to_string(),
                        "./subgraph".to_string(),
                    ),
                )))
            }
            "tap-contracts" | "tap-contracts-deployment" => {
                Some(TaskHandle::TapContracts(Arc::new(TapContractsTask::new(
                    "http://localhost:8545".to_string(),
                    "./tap-contracts".to_string(),
                ))))
            }
            _ => None,
        }
    }

    /// Create a task with specific configuration
    pub fn create_configured_task(
        task_type: &str,
        config: &serde_json::Value,
    ) -> Option<TaskHandle> {
        match task_type {
            "graph-contracts" | "graph-contracts-deployment" => {
                let ethereum_url = config
                    .get("ethereum_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("http://localhost:8545")
                    .to_string();
                let working_dir = config
                    .get("working_dir")
                    .and_then(|v| v.as_str())
                    .unwrap_or("./contracts")
                    .to_string();
                Some(TaskHandle::GraphContracts(Arc::new(
                    GraphContractsTask::new(ethereum_url, working_dir),
                )))
            }
            "subgraph" | "subgraph-deployment" => {
                let graph_node_url = config
                    .get("graph_node_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("http://localhost:8000")
                    .to_string();
                let ipfs_url = config
                    .get("ipfs_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("http://localhost:5001")
                    .to_string();
                let ethereum_url = config
                    .get("ethereum_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("http://localhost:8545")
                    .to_string();
                let working_dir = config
                    .get("working_dir")
                    .and_then(|v| v.as_str())
                    .unwrap_or("./subgraph")
                    .to_string();
                Some(TaskHandle::SubgraphDeploy(Arc::new(
                    SubgraphDeployTask::new(graph_node_url, ipfs_url, ethereum_url, working_dir),
                )))
            }
            "tap-contracts" | "tap-contracts-deployment" => {
                let ethereum_url = config
                    .get("ethereum_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("http://localhost:8545")
                    .to_string();
                let working_dir = config
                    .get("working_dir")
                    .and_then(|v| v.as_str())
                    .unwrap_or("./tap-contracts")
                    .to_string();
                Some(TaskHandle::TapContracts(Arc::new(TapContractsTask::new(
                    ethereum_url, working_dir,
                ))))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[smol_potat::test]
    async fn test_factory_creates_tasks() {
        // Test creating each task type
        let tasks = vec![
            "graph-contracts",
            "subgraph-deployment",
            "tap-contracts-deployment",
        ];

        for task_type in tasks {
            let task = TaskFactory::create_task(task_type);
            assert!(
                task.is_some(),
                "Failed to create task: {}",
                task_type
            );

            // Verify we can call TaskHandle methods
            let task = task.unwrap();
            let result = task.is_completed().await;
            assert!(result.is_ok());
        }
    }

    #[smol_potat::test]
    async fn test_factory_with_config() {
        let config = serde_json::json!({
            "ethereum_url": "http://custom:8545",
            "working_dir": "./custom-contracts"
        });

        let task = TaskFactory::create_configured_task("graph-contracts", &config);
        assert!(task.is_some());
    }

    #[test]
    fn test_unknown_task_type() {
        let task = TaskFactory::create_task("unknown-task");
        assert!(task.is_none());
    }
}