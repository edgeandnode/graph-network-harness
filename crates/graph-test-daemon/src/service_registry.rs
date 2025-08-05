//! Service registry for creating Graph Protocol services by type
//!
//! This module provides a registry that maps service type identifiers
//! to service constructors, enabling dynamic service creation from YAML config.

use harness_core::prelude::*;
use harness_core::service::ServiceWrapper;
use std::collections::HashMap;

use crate::services::{AnvilService, GraphNodeService, IpfsService, PostgresService};

/// Function type for creating service instances
type ServiceConstructor = Box<dyn Fn() -> Box<dyn JsonService> + Send + Sync>;

/// Registry for Graph Protocol services
pub struct ServiceRegistry {
    /// Map from service_type to constructor function
    constructors: HashMap<&'static str, ServiceConstructor>,
}

impl ServiceRegistry {
    /// Create a new service registry with all Graph Protocol services registered
    pub fn new() -> Self {
        let mut registry = Self {
            constructors: HashMap::new(),
        };

        // Register all known Graph Protocol services
        registry.register::<GraphNodeService>();
        registry.register::<AnvilService>();
        registry.register::<PostgresService>();
        registry.register::<IpfsService>();

        registry
    }

    /// Register a service type
    fn register<S>(&mut self)
    where
        S: Service + Default + 'static,
        S::Action: schemars::JsonSchema,
        S::Event: schemars::JsonSchema,
    {
        let service_type = S::service_type();
        self.constructors.insert(
            service_type,
            Box::new(|| {
                let service = S::default();
                let wrapper = ServiceWrapper::new(service);
                Box::new(wrapper)
            }),
        );
    }

    /// Create a service instance by type
    pub fn create_service(&self, service_type: &str) -> Option<Box<dyn JsonService>> {
        self.constructors
            .get(service_type)
            .map(|constructor| constructor())
    }

    /// Get all registered service types
    pub fn registered_types(&self) -> Vec<&'static str> {
        self.constructors.keys().copied().collect()
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_registry() {
        let registry = ServiceRegistry::new();

        // Check that services are registered
        let types = registry.registered_types();
        assert!(types.contains(&"anvil"));
        assert!(types.contains(&"postgres"));
        assert!(types.contains(&"ipfs"));
        assert!(types.contains(&"graph-node"));

        // Test creating a service
        let service = registry.create_service("anvil");
        assert!(service.is_some());

        // Test unknown service type
        let unknown = registry.create_service("unknown");
        assert!(unknown.is_none());
    }
}
