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
    /// Execute the task and wait for completion
    pub async fn execute(&self) -> Result<(), Error> {
        match self {
            TaskHandle::GraphContracts(task) => {
                // Execute the task and consume state stream
                let mut rx = task.execute().await?;
                while let Ok(state) = rx.recv().await {
                    tracing::info!("Graph contracts state: {:?}", state);
                }
                Ok(())
            }
            TaskHandle::SubgraphDeploy(task) => {
                // SubgraphDeployTask needs to be updated to new pattern
                task.deploy().await
            }
            TaskHandle::TapContracts(task) => {
                // Execute the task and consume state stream
                let mut rx = task.execute().await?;
                while let Ok(state) = rx.recv().await {
                    tracing::info!("TAP contracts state: {:?}", state);
                }
                Ok(())
            }
        }
    }

    /// Get the task type
    pub fn task_type(&self) -> &str {
        match self {
            TaskHandle::GraphContracts(_) => "graph-contracts-deployment",
            TaskHandle::SubgraphDeploy(_) => "subgraph-deployment",
            TaskHandle::TapContracts(_) => "tap-contracts-deployment",
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

            // Verify we can get the task type
            let task = task.unwrap();
            assert!(!task.task_type().is_empty());
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
        assert_eq!(task.task_type(), "graph-contracts-deployment");
    }
}
