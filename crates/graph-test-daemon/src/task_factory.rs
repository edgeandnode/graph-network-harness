//! Factory for creating deployment tasks
//!
//! This module provides a factory that creates task instances for various
//! Graph Protocol deployment operations using state machines.

use std::sync::Arc;

use crate::tasks::{GraphContractsTask, SubgraphDeployTask, TapContractsTask};
use harness_core::{Error, config_traits::TaskFromConfig, task::DeploymentTask};
use service_orchestration::TaskConfig;
use std::result::Result;

/// Task handle that can execute tasks
#[derive(Debug)]
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
    pub async fn is_completed(&self) -> Result<bool, Error> {
        match self {
            TaskHandle::GraphContracts(task) => task.is_completed().await,
            TaskHandle::SubgraphDeploy(task) => task.is_completed().await,
            TaskHandle::TapContracts(task) => task.is_completed().await,
        }
    }

    /// Execute the task with default action (DeployAll)
    pub async fn execute(&self) -> Result<(), Error> {
        match self {
            TaskHandle::GraphContracts(task) => {
                // For now, just run the deploy method which uses the state machine
                task.deploy().await
            }
            TaskHandle::SubgraphDeploy(task) => {
                // SubgraphDeployTask needs DeploymentTask implementation
                task.deploy().await
            }
            TaskHandle::TapContracts(task) => {
                // TapContractsTask needs DeploymentTask implementation
                task.deploy().await
            }
        }
    }

    /// Get the task description
    pub fn description(&self) -> &str {
        match self {
            TaskHandle::GraphContracts(task) => task.description(),
            TaskHandle::SubgraphDeploy(_task) => "Deploy subgraphs to Graph Node",
            TaskHandle::TapContracts(_task) => "Deploy TAP contracts",
        }
    }
}

/// Factory for creating deployment tasks
pub struct TaskFactory;

impl TaskFactory {
    /// Create a task from a TaskConfig
    pub fn from_config(config: &TaskConfig) -> Result<TaskHandle, Error> {
        match config.task_type.as_str() {
            "graph-contracts-deployment" => {
                let task = GraphContractsTask::from_config(config)?;
                Ok(TaskHandle::GraphContracts(Arc::new(task)))
            }
            "tap-contracts-deployment" => {
                let task = TapContractsTask::from_config(config)?;
                Ok(TaskHandle::TapContracts(Arc::new(task)))
            }
            "subgraph-deployment" => {
                let task = SubgraphDeployTask::from_config(config)?;
                Ok(TaskHandle::SubgraphDeploy(Arc::new(task)))
            }
            unknown => Err(Error::validation(format!("Unknown task type: {}", unknown))),
        }
    }

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
            "subgraph" | "subgraph-deployment" => Some(TaskHandle::SubgraphDeploy(Arc::new(
                SubgraphDeployTask::new(
                    "http://localhost:8000".to_string(),
                    "http://localhost:5001".to_string(),
                    "http://localhost:8545".to_string(),
                    "./subgraph".to_string(),
                    "test/subgraph".to_string(),
                ),
            ))),
            "tap-contracts" | "tap-contracts-deployment" => {
                Some(TaskHandle::TapContracts(Arc::new(TapContractsTask::new(
                    "http://localhost:8545".to_string(),
                    "./tap-contracts".to_string(),
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
            assert!(task.is_some(), "Failed to create task: {}", task_type);

            // Verify we can call TaskHandle methods
            let task = task.unwrap();
            let result = task.is_completed().await;
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_unknown_task_type() {
        let task = TaskFactory::create_task("unknown-task");
        assert!(task.is_none());
    }

    #[smol_potat::test]
    async fn test_from_config() {
        use service_orchestration::{ProcessCommand, ServiceTarget};
        use std::collections::HashMap;

        let mut env = HashMap::new();
        env.insert("ETHEREUM_URL".to_string(), "http://test:8545".to_string());

        let task_config = TaskConfig {
            task_type: "graph-contracts-deployment".to_string(),
            target: ServiceTarget::Process {
                command: ProcessCommand::Legacy {
                    command: "test".to_string(),
                },
                env,
                working_dir: Some("./test-contracts".to_string()),
            },
            dependencies: Vec::new(),
            config: HashMap::new(),
        };

        let task = TaskFactory::from_config(&task_config);
        assert!(
            task.is_ok(),
            "Failed to create task from TaskConfig: {:?}",
            task
        );

        // Verify the task can be used
        let task = task.unwrap();
        assert_eq!(task.name(), "graph-contracts");
        let is_completed = task.is_completed().await;
        assert!(is_completed.is_ok());
    }
}
