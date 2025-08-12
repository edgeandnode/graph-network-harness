//! JSON service adapter for creating Graph Protocol services with JSON interfaces
//!
//! This module provides an adapter that maps service type identifiers
//! to constructor functions, creating services wrapped in JSON-compatible interfaces
//! for dynamic dispatch from untyped configuration.

use crate::service::{JsonService, Service, ServiceWrapper};
use schemars::JsonSchema;
use std::collections::HashMap;

/// Function type for creating service instances
type ServiceConstructor = Box<dyn Fn() -> Box<dyn JsonService> + Send + Sync>;

/// Adapter for creating JSON-wrapped Graph Protocol services
pub struct JsonServiceAdapter {
    /// Map from service_type to constructor function
    constructors: HashMap<&'static str, ServiceConstructor>,
}

impl JsonServiceAdapter {
    /// Create a new empty JSON service adapter
    pub fn new() -> Self {
        Self {
            constructors: HashMap::new(),
        }
    }

    /// Register a service type with the adapter
    pub fn register<S>(&mut self)
    where
        S: Service + Default + 'static,
        S::Action: JsonSchema,
        S::Event: JsonSchema,
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

impl Default for JsonServiceAdapter {
    fn default() -> Self {
        Self::new()
    }
}

// Tests will be in graph-test-daemon where concrete service types are available
