//! Registry for service executors
//!
//! Maps service targets to appropriate executor implementations

use super::{DockerExecutor, ProcessExecutor, ServiceExecutor};
use crate::{Error, config::ServiceConfig};
use std::collections::HashMap;
use std::sync::Arc;

/// Registry that manages service executors
pub struct ExecutorRegistry {
    /// Registered executors
    executors: HashMap<String, Arc<dyn ServiceExecutor>>,
}

impl ExecutorRegistry {
    /// Create a new executor registry with default executors
    pub fn new() -> Self {
        let mut registry = Self {
            executors: HashMap::new(),
        };

        // Register default executors
        registry.register("process", Arc::new(ProcessExecutor::new()));
        registry.register("docker", Arc::new(DockerExecutor::new()));

        registry
    }

    /// Register a custom executor
    pub fn register(&mut self, name: &str, executor: Arc<dyn ServiceExecutor>) {
        self.executors.insert(name.to_string(), executor);
    }

    /// Find an executor that can handle the given service configuration
    pub fn find_executor(&self, config: &ServiceConfig) -> Result<Arc<dyn ServiceExecutor>, Error> {
        // Try each executor to see which can handle this config
        for executor in self.executors.values() {
            if executor.can_handle(config) {
                return Ok(executor.clone());
            }
        }

        Err(Error::Config(format!(
            "No executor found for service target type: {:?}",
            config.target
        )))
    }

    /// Get a specific executor by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn ServiceExecutor>> {
        self.executors.get(name).cloned()
    }

    /// List all registered executor names
    pub fn list_executors(&self) -> Vec<String> {
        self.executors.keys().cloned().collect()
    }
}

impl Default for ExecutorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServiceTarget;
    use std::collections::HashMap;

    #[test]
    fn test_registry_creation() {
        let registry = ExecutorRegistry::new();
        let executors = registry.list_executors();
        assert!(executors.contains(&"process".to_string()));
        assert!(executors.contains(&"docker".to_string()));
    }

    #[test]
    fn test_find_process_executor() {
        let registry = ExecutorRegistry::new();
        let config = ServiceConfig {
            name: "test".to_string(),
            target: ServiceTarget::Process {
                binary: "test".to_string(),
                args: vec![],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![],
            health_check: None,
        };

        let executor = registry.find_executor(&config);
        assert!(executor.is_ok());
    }

    #[test]
    fn test_find_docker_executor() {
        let registry = ExecutorRegistry::new();
        let config = ServiceConfig {
            name: "test".to_string(),
            target: ServiceTarget::Docker {
                image: "test:latest".to_string(),
                env: HashMap::new(),
                ports: vec![],
                volumes: vec![],
            },
            dependencies: vec![],
            health_check: None,
        };

        let executor = registry.find_executor(&config);
        assert!(executor.is_ok());
    }
}
