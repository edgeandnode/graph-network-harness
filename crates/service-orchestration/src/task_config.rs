//! Task configuration types for deployment tasks
//!
//! This module defines the configuration model for tasks that perform
//! one-time setup operations.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

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
    pub depends_on: Vec<Dependency>,
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
            depends_on: Vec::new(),
            config: HashMap::new(),
        }
    }

    /// Add a dependency
    pub fn with_dependency(mut self, dep: Dependency) -> Self {
        self.depends_on.push(dep);
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

impl StackConfig {
    /// Load configuration from a file
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read config file: {}", e))?;
        serde_yaml::from_str(&content).map_err(|e| format!("Failed to parse config YAML: {}", e))
    }

    /// Load configuration from a reader
    pub fn from_reader<R: std::io::Read>(reader: R) -> Result<Self, String> {
        serde_yaml::from_reader(reader).map_err(|e| format!("Failed to parse config YAML: {}", e))
    }

    /// Inject runtime parameters into all service and task targets
    ///
    /// These params are merged with existing params and can be referenced
    /// using `{param_name}` syntax in templates, layer hosts, health checks, etc.
    /// Substitution is applied immediately after injection.
    ///
    /// Common runtime params:
    /// - `container_host`: IP/hostname of the container running services
    pub fn inject_runtime_params(&mut self, params: HashMap<String, crate::config::ParamValue>) {
        for service_config in self.services.values_mut() {
            service_config
                .orchestration
                .target
                .inject_params(params.clone());
            // Substitute in health check args
            if let Some(health_check) = &mut service_config.orchestration.health_check {
                health_check.substitute_params(&params);
            }
        }
        for task_config in self.tasks.values_mut() {
            task_config.target.inject_params(params.clone());
            task_config.target.substitute_process_fields(&params);
        }
    }
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
                command: crate::config::ProcessCommand::Legacy {
                    command: "npx hardhat deploy".to_string(),
                },
                env: HashMap::from([("NETWORK".to_string(), "localhost".to_string())]),
                ports: HashMap::new(),
                resources: None,
                working_dir: Some("./contracts".to_string()),
                validation: None,
            },
            depends_on: vec![Dependency::Service {
                service: "anvil".to_string(),
            }],
            config: HashMap::from([(
                "deployer_key".to_string(),
                Value::String("0x123...".to_string()),
            )]),
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
            tasks: HashMap::from([(
                "deploy-contracts".to_string(),
                TaskConfig::new(
                    "graph-contracts".to_string(),
                    ServiceTarget::Process {
                        command: crate::config::ProcessCommand::Legacy {
                            command: "hardhat deploy".to_string(),
                        },
                        env: HashMap::new(),
                        ports: HashMap::new(),
                        resources: None,
                        working_dir: None,
                        validation: None,
                    },
                ),
            )]),
        };

        let yaml = serde_yaml::to_string(&stack).expect("Failed to serialize");
        assert!(yaml.contains("tasks:"));
        assert!(yaml.contains("deploy-contracts:"));
    }
}
