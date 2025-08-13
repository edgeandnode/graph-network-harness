//! Service stack system for defining services
//!
//! This module provides traits and utilities for defining services.
//! Services provide functionality that can be orchestrated by the harness.
//! JSON actions for services are generated using the #[json_actions] macro.

use async_channel::Receiver;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::Error;
use std::result::Result;

/// Base trait for services
///
/// Services provide functionality that actions can orchestrate.
/// This trait provides core service identity.
pub trait Service: Send + Sync + 'static {
    /// The service type identifier that links this implementation to YAML service definitions.
    /// YAML services with matching `service_type` will use this implementation for actions.
    fn service_type() -> &'static str
    where
        Self: Sized;

    /// Get the service name
    fn name(&self) -> &str;

    /// Get the service description
    fn description(&self) -> &str;
}

/// Trait for services that emit events
#[async_trait]
pub trait ServiceEvents: Service {
    /// The event type emitted by this service
    type Event: Serialize + Send + JsonSchema;

    /// Get the JSON schema for this service's events
    fn event_schema() -> serde_json::Value
    where
        Self: Sized,
    {
        serde_json::to_value(schemars::schema_for!(Self::Event)).unwrap_or(Value::Null)
    }

    /// Get the event stream for this service
    fn event_stream(&self) -> Receiver<Self::Event>;
}

/// Service state tracking the lifecycle and setup status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServiceState {
    /// Service has not been started
    NotStarted,
    /// Service is starting up
    Starting,
    /// Service is running normally
    Running,
    /// Service requires setup before it can be fully operational
    SetupRequired,
    /// Service is currently performing setup operations
    SettingUp,
    /// Service setup has completed successfully
    SetupComplete,
    /// Service has failed with an error message
    Failed(String),
}

impl ServiceState {
    /// Check if the service is in a healthy running state
    pub fn is_healthy(&self) -> bool {
        matches!(self, ServiceState::Running | ServiceState::SetupComplete)
    }

    /// Check if the service needs setup
    pub fn needs_setup(&self) -> bool {
        matches!(self, ServiceState::SetupRequired)
    }

    /// Check if the service is currently setting up
    pub fn is_setting_up(&self) -> bool {
        matches!(self, ServiceState::SettingUp)
    }
}

/// Trait for services that need one-time setup operations
///
/// Services implementing this trait can perform initialization tasks
/// such as contract deployment, configuration generation, or state setup.
/// 
/// The framework ensures idempotency by calling validate_setup() before
/// perform_setup(). Setup is only performed if validation fails.
#[async_trait]
pub trait ServiceSetup: Service {
    /// Validate that setup is complete and correct.
    ///
    /// This method is called by the framework BEFORE perform_setup() to check
    /// if setup is already done (idempotency check). 
    ///
    /// Returns Ok(()) if setup is complete and valid - perform_setup() will be skipped.
    /// Returns an error if setup is incomplete or invalid - perform_setup() will be called.
    ///
    /// Implementers should check both:
    /// - Existence (is the database/contract/config present?)
    /// - Correctness (is it the right version/configuration?)
    async fn validate_setup(&self) -> Result<(), Error>;

    /// Perform the one-time setup operations for this service.
    ///
    /// This method is only called if validate_setup() returned an error,
    /// indicating setup is needed. The framework ensures idempotency by
    /// calling validate_setup() first.
    ///
    /// After this method completes, validate_setup() will be called again
    /// to confirm successful setup.
    async fn perform_setup(&self) -> Result<(), Error>;
}

/// Trait for services that track their state
///
/// This trait provides state monitoring capabilities for services,
/// enabling coordination and waiting for specific states.
#[async_trait]
pub trait StatefulService: Service {
    /// Get the current state of the service
    async fn get_state(&self) -> Result<ServiceState, Error>;

    /// Wait for the service to reach a target state with timeout
    ///
    /// This method will poll the service state until it reaches the target
    /// state or the timeout is exceeded.
    ///
    /// Note: The implementation should provide appropriate delays between checks
    /// using the runtime-specific timer mechanism.
    async fn wait_for_state(&self, target: ServiceState, timeout: Duration) -> Result<(), Error>;

    /// Default implementation using polling without delays
    ///
    /// Services should override this with runtime-specific delay mechanisms
    async fn poll_for_state(&self, target: ServiceState, timeout: Duration) -> Result<(), Error> {
        let start = std::time::Instant::now();

        while start.elapsed() < timeout {
            let current = self.get_state().await?;
            if current == target {
                return Ok(());
            }
        }

        Err(Error::service_type(format!(
            "Timeout waiting for service {} to reach state {:?}",
            self.name(),
            target
        )))
    }
}

/// Descriptor for an action that a service can perform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionDescriptor {
    /// Action name
    pub name: String,

    /// Action description
    pub description: String,

    /// JSON schema for the action input
    pub input_schema: Value,

    /// JSON schema for the event type
    pub event_schema: Value,
}

/// Trait for services that work with JSON (used for dynamic dispatch)
///
/// This trait is object-safe and does not use generic parameters.
/// It's typically implemented automatically when using the #[json_actions] macro.
#[async_trait]
pub trait JsonService: Send + Sync {
    /// Get the service name
    fn name(&self) -> &str;

    /// Get the service description
    fn description(&self) -> &str;

    /// Get available actions
    fn available_actions(&self) -> Vec<ActionDescriptor>;

    /// Dispatch an action using JSON input
    /// Returns a receiver for JSON events and a future that must be spawned to perform the conversion
    async fn dispatch_json(
        &self,
        action_name: &str,
        input: Value,
    ) -> Result<(Receiver<Value>, Pin<Box<dyn Future<Output = ()> + Send>>), Error>;

    /// Get the current state of the service
    async fn get_state(&self) -> Result<ServiceState, Error> {
        Ok(ServiceState::Running)
    }

    /// Wait for the service to reach a target state with timeout
    async fn wait_for_state(&self, target: ServiceState, timeout: Duration) -> Result<(), Error> {
        let current = self.get_state().await?;
        if current == target {
            Ok(())
        } else {
            Err(Error::service_type(format!(
                "Service {} is in state {:?}, not {:?}",
                self.name(),
                current,
                target
            )))
        }
    }

    /// Check if this service implements ServiceSetup
    fn has_setup(&self) -> bool {
        false
    }
    
    /// Check if this service implements ServiceEvents
    fn has_events(&self) -> bool {
        false
    }
    
    /// Get the event schema if this service emits events
    fn event_schema(&self) -> Option<Value> {
        None
    }

    /// Validate setup if this service implements ServiceSetup
    async fn validate_setup(&self) -> Result<(), Error> {
        Err(Error::service_type("Service does not implement ServiceSetup"))
    }

    /// Perform setup if this service implements ServiceSetup
    async fn perform_setup(&self) -> Result<(), Error> {
        Err(Error::service_type("Service does not implement ServiceSetup"))
    }
}

/// Adapter that wraps services with JSON actions into JsonService
/// 
/// This adapter is generic over the service type and calls dispatch_json_action
/// directly without needing a function pointer.
pub struct JsonServiceAdapter<S> {
    service: Arc<S>,
    registry: crate::action::JsonActionRegistry,
}

impl<S> JsonServiceAdapter<S>
where
    S: Service + crate::action::ServiceJsonActions + 'static,
{
    /// Create a new adapter for a service with JSON actions
    pub fn new(service: S) -> Result<Self, Error> {
        let service = Arc::new(service);
        let mut registry = crate::action::JsonActionRegistry::new();
        
        // Register the service's actions
        S::register_actions(&mut registry)?;
        
        Ok(Self { service, registry })
    }
}

/// Trait that services must implement to have dispatch_json_action
/// This is implemented by the #[json_actions] macro
#[async_trait]
pub trait HasDispatchJson: Send + Sync {
    /// Dispatch a JSON action to this service
    async fn dispatch_json_action(
        &self,
        action_name: &str,
        input: Value,
    ) -> Result<Value, Error>;
}

#[async_trait]
impl<S> JsonService for JsonServiceAdapter<S>
where
    S: Service + crate::action::ServiceJsonActions + HasDispatchJson + Send + Sync + 'static,
{
    fn name(&self) -> &str {
        self.service.name()
    }
    
    fn description(&self) -> &str {
        self.service.description()
    }
    
    fn available_actions(&self) -> Vec<ActionDescriptor> {
        self.registry.action_names().iter().map(|name| {
            let (input_schema, response_schema) = self.registry.get_schema(name)
                .cloned()
                .unwrap_or((Value::Null, Value::Null));
            
            ActionDescriptor {
                name: name.clone(),
                description: self.service.description().to_string(),
                input_schema,
                event_schema: response_schema,
            }
        }).collect()
    }
    
    async fn dispatch_json(
        &self,
        action_name: &str,
        input: Value,
    ) -> Result<(Receiver<Value>, Pin<Box<dyn Future<Output = ()> + Send>>), Error> {
        // Create a channel for the response
        let (tx, rx) = async_channel::bounded(1);
        
        // Call dispatch_json_action directly on the service
        let service = self.service.clone();
        let action_name = action_name.to_string();
        
        let dispatcher = async move {
            match service.dispatch_json_action(&action_name, input).await {
                Ok(result) => {
                    let _ = tx.send(result).await;
                }
                Err(e) => {
                    // Send error as JSON
                    let error_json = serde_json::json!({
                        "error": e.to_string()
                    });
                    let _ = tx.send(error_json).await;
                }
            }
        };
        
        Ok((rx, Box::pin(dispatcher)))
    }
    
    // Note: ServiceSetup detection would require additional trait bounds
    // For now, services that implement ServiceSetup should provide their own JsonService wrapper
    // that properly implements has_setup(), validate_setup(), and perform_setup()
}

/// Registry of JSON-wrapped services available in a daemon
///
/// The JsonServiceRegistry manages a collection of services that have been
/// wrapped to provide a JSON interface. Services are stored as trait objects
/// to allow different service types with unified JSON-based interaction.
pub struct JsonServiceRegistry {
    services: HashMap<String, Box<dyn JsonService>>,
    service_types: HashSet<String>,
}

impl JsonServiceRegistry {
    /// Create a new empty service registry
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
            service_types: HashSet::new(),
        }
    }
    
    /// Register a service with automatic JSON wrapping
    ///
    /// This method automatically wraps the service in a JsonServiceAdapter
    pub fn register<S>(&mut self, instance_name: String, service: S) -> Result<(), Error>
    where
        S: Service + crate::action::ServiceJsonActions + HasDispatchJson + 'static,
    {
        if self.services.contains_key(&instance_name) {
            return Err(Error::service_type(format!(
                "Service instance '{instance_name}' already registered"
            )));
        }

        // Track the service type
        self.service_types.insert(S::service_type().to_string());

        let adapter = JsonServiceAdapter::new(service)?;
        self.services.insert(instance_name, Box::new(adapter));
        Ok(())
    }

    /// Register a service in the registry
    pub fn register_json_service(
        &mut self,
        instance_name: String,
        service: Box<dyn JsonService>,
    ) -> Result<(), Error> {
        if self.services.contains_key(&instance_name) {
            return Err(Error::service_type(format!(
                "Service instance '{instance_name}' already registered"
            )));
        }

        self.services.insert(instance_name, service);
        Ok(())
    }

    /// Get a service by instance name
    pub fn get(&self, instance_name: &str) -> Option<&dyn JsonService> {
        self.services.get(instance_name).map(|s| s.as_ref())
    }

    /// List all registered service instances
    pub fn list(&self) -> Vec<(&str, &dyn JsonService)> {
        self.services
            .iter()
            .map(|(name, service)| (name.as_str(), service.as_ref()))
            .collect()
    }

    /// Get all available actions across all services
    pub fn all_actions(&self) -> Vec<(String, ActionDescriptor)> {
        self.services
            .iter()
            .flat_map(|(instance, service)| {
                service
                    .available_actions()
                    .into_iter()
                    .map(move |action| (instance.clone(), action))
            })
            .collect()
    }
    
    /// Get all service type identifiers
    pub fn list_types(&self) -> Vec<&str> {
        self.service_types.iter().map(|s| s.as_str()).collect()
    }

    /// Dispatch an action to a specific service instance
    pub async fn dispatch(
        &self,
        instance_name: &str,
        action_name: &str,
        input: Value,
    ) -> Result<(Receiver<Value>, Pin<Box<dyn Future<Output = ()> + Send>>), Error> {
        let service = self.get(instance_name).ok_or_else(|| {
            Error::service_type(format!("Service instance '{instance_name}' not found"))
        })?;

        service.dispatch_json(action_name, input).await
    }
}

impl Default for JsonServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{JsonAction, JsonActionRegistry, ServiceJsonActions};
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    // Test service
    struct TestService {
        value: std::sync::Arc<std::sync::Mutex<i32>>,
        event_tx: async_channel::Sender<TestEvent>,
        event_rx: async_channel::Receiver<TestEvent>,
    }

    impl TestService {
        fn new() -> Self {
            let (tx, rx) = async_channel::unbounded();
            Self {
                value: std::sync::Arc::new(std::sync::Mutex::new(0)),
                event_tx: tx,
                event_rx: rx,
            }
        }

        async fn add_value(&self, amount: i32) -> Result<i32, Error> {
            let mut val = self.value.lock().unwrap();
            *val += amount;
            let new_val = *val;
            
            self.event_tx.send(TestEvent {
                value: new_val,
            }).await.unwrap();
            
            Ok(new_val)
        }
    }

    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    struct TestEvent {
        value: i32,
    }

    impl Service for TestService {
        fn service_type() -> &'static str {
            "test-service"
        }

        fn name(&self) -> &str {
            "test"
        }

        fn description(&self) -> &str {
            "Test service"
        }
    }

    #[async_trait]
    impl ServiceEvents for TestService {
        type Event = TestEvent;

        fn event_stream(&self) -> Receiver<Self::Event> {
            self.event_rx.clone()
        }
    }

    // Manually define an action for testing (normally generated by macro)
    #[derive(Debug, Deserialize, Serialize, JsonSchema)]
    struct AddValueAction {
        amount: i32,
    }

    #[async_trait]
    impl JsonAction<TestService> for AddValueAction {
        type Response = i32;

        fn action_name() -> &'static str {
            "add_value"
        }

        async fn execute(self, service: &TestService) -> Result<Self::Response, Error> {
            service.add_value(self.amount).await
        }
    }

    impl ServiceJsonActions for TestService {
        fn register_actions(registry: &mut JsonActionRegistry) -> Result<(), Error> {
            registry.register::<AddValueAction, TestService>()?;
            Ok(())
        }
    }

    #[test]
    fn test_service_state() {
        assert!(ServiceState::Running.is_healthy());
        assert!(ServiceState::SetupComplete.is_healthy());
        assert!(!ServiceState::Failed("error".to_string()).is_healthy());
        assert!(ServiceState::SetupRequired.needs_setup());
        assert!(ServiceState::SettingUp.is_setting_up());
    }

    #[smol_potat::test]
    async fn test_json_action_execution() {
        let service = TestService::new();
        let action = AddValueAction { amount: 5 };
        let result = action.execute(&service).await.unwrap();
        assert_eq!(result, 5);

        // Check event was emitted
        let event = service.event_stream().recv().await.unwrap();
        assert_eq!(event.value, 5);
    }
}