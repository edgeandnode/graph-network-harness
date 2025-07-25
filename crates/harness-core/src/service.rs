//! Service type system for defining domain-specific services
//!
//! This module provides traits and utilities for defining custom service types
//! that extend the base harness functionality. Domain-specific service types
//! can be converted to standard ServiceConfig for execution.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

use crate::{Error, Result};
use service_orchestration::ServiceConfig;

/// Trait for custom service types that can be converted to ServiceConfig
#[async_trait]
pub trait ServiceType: Send + Sync + Clone + 'static {
    /// The name of this service type (e.g., "graph-node", "anvil-blockchain")
    fn type_name() -> &'static str
    where
        Self: Sized;
    
    /// Convert this service type to a standard ServiceConfig
    fn to_service_config(&self, name: String) -> Result<ServiceConfig>;
    
    /// Validate the service configuration
    fn validate(&self) -> Result<()> {
        Ok(())
    }
    
    /// Transform the service before starting (optional)
    async fn pre_start(&mut self) -> Result<()> {
        Ok(())
    }
    
    /// Get runtime fields that are populated after service starts
    fn runtime_fields(&self) -> HashMap<String, Value> {
        HashMap::new()
    }
}

/// Registry for managing service types
#[derive(Default)]
pub struct ServiceTypeRegistry {
    types: std::collections::HashSet<String>,
}

impl ServiceTypeRegistry {
    /// Create a new service type registry
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Register a service type
    pub fn register<T: ServiceType>(&mut self) -> Result<()> {
        let name = T::type_name();
        
        if self.types.contains(name) {
            return Err(Error::service_type(format!(
                "Service type '{}' already registered",
                name
            )));
        }
        
        self.types.insert(name.to_string());
        Ok(())
    }
    
    /// Check if a service type is registered
    pub fn has_type(&self, name: &str) -> bool {
        self.types.contains(name)
    }
    
    /// Get all registered service type names
    pub fn list_types(&self) -> Vec<&str> {
        self.types.iter().map(|s| s.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use service_orchestration::{ServiceTarget, HealthCheck};
    
    #[derive(Debug, Clone)]
    struct TestService {
        name: String,
    }
    
    #[async_trait]
    impl ServiceType for TestService {
        fn type_name() -> &'static str {
            "test"
        }
        
        fn to_service_config(&self, name: String) -> Result<ServiceConfig> {
            Ok(ServiceConfig {
                name,
                target: ServiceTarget::Process {
                    binary: "echo".to_string(),
                    args: vec!["hello".to_string()],
                    env: HashMap::new(),
                    working_dir: None,
                },
                dependencies: vec![],
                health_check: None,
            })
        }
    }
    
    #[test]
    fn test_service_type_registry() {
        let mut registry = ServiceTypeRegistry::new();
        
        // Register a service type
        registry.register::<TestService>().unwrap();
        
        // Check registration
        assert!(registry.has_type("test"));
        assert_eq!(registry.list_types(), vec!["test"]);
        
        // Try to register again (should fail)
        let result = registry.register::<TestService>();
        assert!(result.is_err());
    }
    
    #[test]
    fn test_service_type_conversion() {
        let test_service = TestService {
            name: "my-test".to_string(),
        };
        
        let config = test_service.to_service_config("test-service".to_string()).unwrap();
        assert_eq!(config.name, "test-service");
        
        match config.target {
            ServiceTarget::Process { binary, .. } => {
                assert_eq!(binary, "echo");
            }
            _ => panic!("Expected Process target"),
        }
    }
}