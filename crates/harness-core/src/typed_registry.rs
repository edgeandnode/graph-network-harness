//! Type-safe registry implementations that store concrete types
//!
//! These registries store services and tasks as concrete types using Any,
//! allowing typed access internally while supporting JSON serialization
//! only at boundaries (like WebSocket communication).

use async_channel::Receiver;
use serde_json::Value;
use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::Error;
use crate::service::Service;
use crate::service::{ActionDescriptor, JsonService, ServiceState};
use crate::task::DeploymentTask;
use crate::task::JsonTask;
use async_trait::async_trait;
use std::result::Result;
use std::time::Duration;

/// Metadata about a registered service
#[derive(Clone, Debug)]
pub struct ServiceMetadata {
    /// The service type identifier
    pub service_type: String,
    /// Whether the service has setup capability
    pub has_setup: bool,
    /// Whether the service emits events
    pub has_events: bool,
}

impl ServiceMetadata {
    /// Create metadata from a service type
    pub fn from<S: Service>() -> Self {
        Self {
            service_type: S::SERVICE_TYPE.to_string(),
            has_setup: false,  // Would need additional trait bounds to detect
            has_events: false, // Would need additional trait bounds to detect
        }
    }
}

/// Type-safe registry for services that preserves concrete types
pub struct TypedServiceRegistry {
    /// Services stored as concrete types wrapped in Arc
    services: HashMap<String, Arc<dyn Any + Send + Sync>>,
    /// Metadata about each service for introspection
    metadata: HashMap<String, ServiceMetadata>,
}

impl TypedServiceRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    /// Register a concrete service
    pub fn register<S>(&mut self, name: String, service: S) -> Result<(), Error>
    where
        S: Service + 'static,
    {
        if self.services.contains_key(&name) {
            return Err(Error::service_type(format!(
                "Service instance '{}' already registered",
                name
            )));
        }

        let arc: Arc<S> = Arc::new(service);
        self.services
            .insert(name.clone(), arc as Arc<dyn Any + Send + Sync>);
        self.metadata.insert(name, ServiceMetadata::from::<S>());
        Ok(())
    }

    /// Get a typed service reference
    pub fn get_service<S>(&self, name: &str) -> Option<Arc<S>>
    where
        S: Service + 'static,
    {
        self.services.get(name)?.clone().downcast::<S>().ok()
    }

    /// Get an untyped service reference (for creating JSON adapters)
    pub fn get_service_any(&self, name: &str) -> Option<Arc<dyn Any + Send + Sync>> {
        self.services.get(name).cloned()
    }

    /// Get metadata about a service
    pub fn get_metadata(&self, name: &str) -> Option<&ServiceMetadata> {
        self.metadata.get(name)
    }

    /// List all registered service names
    pub fn list_services(&self) -> Vec<&str> {
        self.services.keys().map(|s| s.as_str()).collect()
    }

    /// List all service types
    pub fn list_types(&self) -> Vec<&str> {
        let mut types: Vec<_> = self
            .metadata
            .values()
            .map(|m| m.service_type.as_str())
            .collect();
        types.sort();
        types.dedup();
        types
    }
}

impl Default for TypedServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Metadata about a registered task
#[derive(Clone, Debug)]
pub struct TaskMetadata {
    /// The task type identifier
    pub task_type: String,
}

impl TaskMetadata {
    /// Create metadata from a task type
    pub fn from<T: DeploymentTask>() -> Self {
        Self {
            task_type: T::TASK_TYPE.to_string(),
        }
    }
}

/// Type-safe registry for tasks that preserves concrete types
pub struct TypedTaskRegistry {
    /// Tasks stored as concrete types wrapped in Arc
    tasks: HashMap<String, Arc<dyn Any + Send + Sync>>,
    /// Metadata about each task for introspection
    metadata: HashMap<String, TaskMetadata>,
}

impl TypedTaskRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    /// Register a concrete task
    pub fn register<T>(&mut self, name: String, task: T) -> Result<(), Error>
    where
        T: DeploymentTask + 'static,
    {
        if self.tasks.contains_key(&name) {
            return Err(Error::service_type(format!(
                "Task instance '{}' already registered",
                name
            )));
        }

        let arc: Arc<T> = Arc::new(task);
        self.tasks
            .insert(name.clone(), arc as Arc<dyn Any + Send + Sync>);
        self.metadata.insert(name, TaskMetadata::from::<T>());
        Ok(())
    }

    /// Get a typed task reference
    pub fn get_task<T>(&self, name: &str) -> Option<Arc<T>>
    where
        T: DeploymentTask + 'static,
    {
        self.tasks.get(name)?.clone().downcast::<T>().ok()
    }

    /// Get an untyped task reference (for creating JSON adapters)
    pub fn get_task_any(&self, name: &str) -> Option<Arc<dyn Any + Send + Sync>> {
        self.tasks.get(name).cloned()
    }

    /// Get metadata about a task
    pub fn get_metadata(&self, name: &str) -> Option<&TaskMetadata> {
        self.metadata.get(name)
    }

    /// List all registered task names
    pub fn list_tasks(&self) -> Vec<&str> {
        self.tasks.keys().map(|s| s.as_str()).collect()
    }

    /// List all task types
    pub fn list_types(&self) -> Vec<&str> {
        let mut types: Vec<_> = self
            .metadata
            .values()
            .map(|m| m.task_type.as_str())
            .collect();
        types.sort();
        types.dedup();
        types
    }
}

impl Default for TypedTaskRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to create a JSON service adapter from an Any reference
///
/// This is used at the WebSocket boundary to create JSON adapters on-demand
/// from the concrete types stored in the TypedServiceRegistry.
pub struct JsonAdapterFactory;

impl JsonAdapterFactory {
    /// Try to create a JSON service wrapper from an Any reference
    ///
    /// This requires the service to implement the necessary traits for JSON serialization.
    /// The actual implementation would need to be generated or use a registry of known types.
    pub fn create_service_adapter(
        service_any: Arc<dyn Any + Send + Sync>,
        service_type: &str,
    ) -> Result<Box<dyn JsonService>, Error> {
        // This would need to be implemented with a type registry or macro-generated code
        // For now, return an error indicating the type is not registered
        Err(Error::service_type(format!(
            "No JSON adapter factory registered for service type '{}'",
            service_type
        )))
    }

    /// Try to create a JSON task wrapper from an Any reference
    pub fn create_task_adapter(
        task_any: Arc<dyn Any + Send + Sync>,
        task_type: &str,
    ) -> Result<Box<dyn JsonTask>, Error> {
        // This would need to be implemented with a type registry or macro-generated code
        // For now, return an error indicating the type is not registered
        Err(Error::service_type(format!(
            "No JSON adapter factory registered for task type '{}'",
            task_type
        )))
    }
}

/// Dynamic JSON service wrapper that creates adapters on-demand
///
/// This wrapper holds a reference to the typed registry and creates
/// JSON adapters only when needed for dispatch.
pub struct DynamicJsonService {
    name: String,
    service_any: Arc<dyn Any + Send + Sync>,
    metadata: ServiceMetadata,
}

impl DynamicJsonService {
    /// Create a new dynamic JSON service wrapper
    pub fn new(
        name: String,
        service_any: Arc<dyn Any + Send + Sync>,
        metadata: ServiceMetadata,
    ) -> Self {
        Self {
            name,
            service_any,
            metadata,
        }
    }
}

#[async_trait]
impl JsonService for DynamicJsonService {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "Dynamic service wrapper"
    }

    fn available_actions(&self) -> Vec<ActionDescriptor> {
        // Would need to get this from metadata or a registry
        vec![]
    }

    async fn dispatch_json(
        &self,
        action_name: &str,
        input: Value,
    ) -> Result<(Receiver<Value>, Pin<Box<dyn Future<Output = ()> + Send>>), Error> {
        // Try to create an adapter and dispatch through it
        let adapter = JsonAdapterFactory::create_service_adapter(
            self.service_any.clone(),
            &self.metadata.service_type,
        )?;
        adapter.dispatch_json(action_name, input).await
    }

    async fn get_state(&self) -> Result<ServiceState, Error> {
        Ok(ServiceState::Running)
    }

    async fn wait_for_state(&self, target: ServiceState, timeout: Duration) -> Result<(), Error> {
        let current = self.get_state().await?;
        if current == target {
            Ok(())
        } else {
            Err(Error::service_type(format!(
                "Service {} is in state {:?}, not {:?}",
                self.name, current, target
            )))
        }
    }

    fn has_setup(&self) -> bool {
        self.metadata.has_setup
    }

    fn has_events(&self) -> bool {
        self.metadata.has_events
    }

    fn event_schema(&self) -> Option<Value> {
        None
    }

    async fn validate_setup(&self) -> Result<(), Error> {
        if self.has_setup() {
            // Would need to dispatch to the actual service
            Err(Error::service_type(
                "Setup validation not implemented for dynamic wrapper",
            ))
        } else {
            Err(Error::service_type(
                "Service does not implement ServiceSetup",
            ))
        }
    }

    async fn perform_setup(&self) -> Result<(), Error> {
        if self.has_setup() {
            // Would need to dispatch to the actual service
            Err(Error::service_type(
                "Setup not implemented for dynamic wrapper",
            ))
        } else {
            Err(Error::service_type(
                "Service does not implement ServiceSetup",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test service for validation
    struct TestService {
        name: String,
    }

    impl Service for TestService {
        const SERVICE_TYPE: &'static str = "test-service";

        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "Test service"
        }
    }

    #[test]
    fn test_typed_service_registry() {
        let mut registry = TypedServiceRegistry::new();

        let service = TestService {
            name: "test1".to_string(),
        };

        // Register the service
        registry.register("test1".to_string(), service).unwrap();

        // Get it back with the correct type
        let retrieved = registry.get_service::<TestService>("test1");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name(), "test1");

        // Check metadata
        let metadata = registry.get_metadata("test1");
        assert!(metadata.is_some());
        assert_eq!(metadata.unwrap().service_type, "test-service");

        // List services
        assert_eq!(registry.list_services().len(), 1);
        assert_eq!(registry.list_types().len(), 1);
    }

    #[test]
    fn test_duplicate_registration() {
        let mut registry = TypedServiceRegistry::new();

        let service1 = TestService {
            name: "test1".to_string(),
        };
        let service2 = TestService {
            name: "test2".to_string(),
        };

        // First registration should succeed
        registry.register("test".to_string(), service1).unwrap();

        // Second registration with same name should fail
        let result = registry.register("test".to_string(), service2);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("already registered")
        );
    }

    #[test]
    fn test_wrong_type_downcast() {
        let mut registry = TypedServiceRegistry::new();

        struct OtherService;
        impl Service for OtherService {
            const SERVICE_TYPE: &'static str = "other";
            fn name(&self) -> &str {
                "other"
            }
            fn description(&self) -> &str {
                "Other service"
            }
        }

        let service = TestService {
            name: "test".to_string(),
        };

        registry.register("test".to_string(), service).unwrap();

        // Try to get it as wrong type
        let retrieved = registry.get_service::<OtherService>("test");
        assert!(retrieved.is_none());
    }
}
