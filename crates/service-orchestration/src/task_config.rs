//! Task configuration types for deployment tasks
//!
//! This module defines the configuration model for tasks that perform
//! one-time setup operations.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use serde_json::Value;

use crate::config::{Dependency, ServiceConfig, ServiceTarget};

/// Configuration for a deployment task
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskConfig {
    /// The task type that links to task implementations (e.g., "graph-contracts-deployment")
    pub task_type: String,
    /// Where and how to run the task
    pub target: ServiceTarget,
    /// Services and tasks this task depends on
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    /// Task-specific configuration parameters
    #[serde(default)]
    pub config: HashMap<String, Value>,
}

impl TaskConfig {
    /// Create a new task configuration
    pub fn new(task_type: String, target: ServiceTarget) -> Self {
        Self {
            task_type,
            target,
            dependencies: Vec::new(),
            config: HashMap::new(),
        }
    }

    /// Add a dependency
    pub fn with_dependency(mut self, dep: Dependency) -> Self {
        self.dependencies.push(dep);
        self
    }

    /// Add a configuration parameter
    pub fn with_config_param(mut self, key: String, value: Value) -> Self {
        self.config.insert(key, value);
        self
    }
}

/// Configuration for a service instance in a stack
/// 
/// This extends the generic service-orchestration config with a service_type
/// field that links the runtime configuration to action implementations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInstanceConfig {
    /// The service type that links to action implementations (e.g., "postgres", "anvil")
    pub service_type: String,
    /// Generic orchestration configuration from service-orchestration crate
    #[serde(flatten)]
    pub orchestration: ServiceConfig,
}

/// Extended stack configuration that includes both services and tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackConfig {
    /// Stack name
    pub name: String,
    /// Stack description
    pub description: Option<String>,
    /// Service instances in this stack
    #[serde(default)]
    pub services: HashMap<String, ServiceInstanceConfig>,
    /// Task instances in this stack
    #[serde(default)]
    pub tasks: HashMap<String, TaskConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServiceTarget;

    #[test]
    fn test_task_config_serialization() {
        let task = TaskConfig {
            task_type: "graph-contracts-deployment".to_string(),
            target: ServiceTarget::Process {
                binary: "npx".to_string(),
                args: vec!["hardhat".to_string(), "deploy".to_string()],
                env: HashMap::from([("NETWORK".to_string(), "localhost".to_string())]),
                working_dir: Some("./contracts".to_string()),
            },
            dependencies: vec![
                Dependency::Service {
                    service: "anvil".to_string(),
                },
            ],
            config: HashMap::from([
                ("deployer_key".to_string(), Value::String("0x123...".to_string())),
            ]),
        };

        let yaml = serde_yaml::to_string(&task).expect("Failed to serialize");
        let deserialized: TaskConfig = serde_yaml::from_str(&yaml).expect("Failed to deserialize");
        assert_eq!(task, deserialized);
    }

    #[test]
    fn test_stack_config_with_tasks() {
        let stack = StackConfig {
            name: "test-stack".to_string(),
            description: Some("A test stack with tasks".to_string()),
            services: HashMap::new(),
            tasks: HashMap::from([
                ("deploy-contracts".to_string(), TaskConfig::new(
                    "graph-contracts".to_string(),
                    ServiceTarget::Process {
                        binary: "hardhat".to_string(),
                        args: vec!["deploy".to_string()],
                        env: HashMap::new(),
                        working_dir: None,
                    },
                )),
            ]),
        };

        let yaml = serde_yaml::to_string(&stack).expect("Failed to serialize");
        assert!(yaml.contains("tasks:"));
        assert!(yaml.contains("deploy-contracts:"));
    }
}