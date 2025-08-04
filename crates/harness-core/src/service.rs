//! Service stack system for defining services with actions
//!
//! This module provides traits and utilities for defining services that can
//! perform actions. Services are strongly typed with their action and event
//! types, while a wrapper layer handles JSON serialization for wire protocol.

use async_channel::Receiver;
use async_runtime_compat::Spawner;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::{Error, Result};

/// Trait for services that can perform actions
///
/// Services are strongly typed with their Action and Event types.
/// This trait focuses on domain logic without JSON concerns.
#[async_trait]
pub trait Service: Send + Sync + 'static {
    /// The input type for actions this service can perform
    type Action: DeserializeOwned + Send + JsonSchema;

    /// The event type emitted during action execution
    type Event: Serialize + Send;

    /// The service type identifier that links this implementation to YAML service definitions.
    /// YAML services with matching `service_type` will use this implementation for actions.
    fn service_type() -> &'static str
    where
        Self: Sized;

    /// Get the service name
    fn name(&self) -> &str;

    /// Get the service description
    fn description(&self) -> &str;

    /// Get the JSON schema for this service's actions
    fn action_schema() -> serde_json::Value
    where
        Self: Sized,
    {
        serde_json::to_value(schemars::schema_for!(Self::Action)).unwrap_or(Value::Null)
    }

    /// Execute an action, returning a stream of events
    async fn dispatch_action(&self, action: Self::Action) -> Result<Receiver<Self::Event>>;
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
#[async_trait]
pub trait ServiceSetup: Service {
    /// Check if the service setup has already been completed
    ///
    /// This enables idempotency - setup operations can be safely retried
    /// without causing duplicate work or conflicts.
    async fn is_setup_complete(&self) -> Result<bool>;

    /// Perform the one-time setup operations for this service
    ///
    /// This method should be idempotent and safe to call multiple times.
    /// It should check `is_setup_complete()` before performing work.
    async fn perform_setup(&self) -> Result<()>;

    /// Validate that setup completed successfully
    ///
    /// This method can perform additional checks to ensure that setup
    /// operations completed correctly (e.g., contract address validation).
    async fn validate_setup(&self) -> Result<()> {
        Ok(())
    }
}

/// Trait for services that track their state
///
/// This trait provides state monitoring capabilities for services,
/// enabling coordination and waiting for specific states.
#[async_trait]
pub trait StatefulService: Service {
    /// Get the current state of the service
    async fn get_state(&self) -> Result<ServiceState>;

    /// Wait for the service to reach a target state with timeout
    ///
    /// This method will poll the service state until it reaches the target
    /// state or the timeout is exceeded.
    ///
    /// Note: The implementation should provide appropriate delays between checks
    /// using the runtime-specific timer mechanism.
    async fn wait_for_state(&self, target: ServiceState, timeout: Duration) -> Result<()>;

    /// Default implementation using polling without delays
    ///
    /// Services should override this with runtime-specific delay mechanisms
    async fn poll_for_state(&self, target: ServiceState, timeout: Duration) -> Result<()> {
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

/// Wrapper that adds JSON serialization to any Service
///
/// This wrapper handles conversion between JSON and typed values,
/// allowing services to work with their native types while supporting
/// wire protocol communication.
pub struct ServiceWrapper<S>
where
    S: Service,
    S::Action: JsonSchema,
    S::Event: JsonSchema,
{
    inner: S,
    action_schema: Value,
    event_schema: Value,
}

impl<S> ServiceWrapper<S>
where
    S: Service,
    S::Action: JsonSchema,
    S::Event: JsonSchema,
{
    /// Create a new service wrapper
    pub fn new(service: S) -> Self {
        let action_schema = schemars::schema_for!(S::Action);
        let event_schema = schemars::schema_for!(S::Event);

        Self {
            inner: service,
            action_schema: serde_json::to_value(action_schema).unwrap(),
            event_schema: serde_json::to_value(event_schema).unwrap(),
        }
    }

    /// Get available actions with their schemas
    pub fn available_actions(&self) -> Vec<ActionDescriptor> {
        // In a real implementation, we'd have a way to enumerate actions
        // For now, return a single action descriptor
        vec![ActionDescriptor {
            name: "default".to_string(),
            description: self.inner.description().to_string(),
            input_schema: self.action_schema.clone(),
            event_schema: self.event_schema.clone(),
        }]
    }

    /// Dispatch an action using JSON input, returning typed events
    pub async fn dispatch_json(
        &self,
        _action_name: &str,
        input: Value,
    ) -> Result<Receiver<S::Event>> {
        // Deserialize JSON to typed action
        let action: S::Action = serde_json::from_value(input)
            .map_err(|e| Error::service_type(format!("Failed to deserialize action: {}", e)))?;

        // Execute the typed action and return the event receiver directly
        self.inner.dispatch_action(action).await
    }

    /// Get event schema for JSON conversion
    pub fn event_schema(&self) -> &Value {
        &self.event_schema
    }
}

/// Trait for services that work with JSON (used for dynamic dispatch)
///
/// Note: This trait is object-safe and does not use generic parameters.
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
    ) -> Result<(Receiver<Value>, Pin<Box<dyn Future<Output = ()> + Send>>)>;

    /// Get the current state of the service
    async fn get_state(&self) -> Result<ServiceState> {
        Ok(ServiceState::Running)
    }

    /// Wait for the service to reach a target state with timeout
    async fn wait_for_state(&self, target: ServiceState, timeout: Duration) -> Result<()> {
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

    /// Perform setup if this service implements ServiceSetup
    async fn perform_setup(&self) -> Result<()> {
        Ok(())
    }

    /// Check if setup is complete
    async fn is_setup_complete(&self) -> Result<bool> {
        Ok(true)
    }
}

/// Blanket implementation for wrapped services
#[async_trait]
impl<S> JsonService for ServiceWrapper<S>
where
    S: Service + 'static,
    S::Action: JsonSchema,
    S::Event: JsonSchema,
{
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn available_actions(&self) -> Vec<ActionDescriptor> {
        self.available_actions()
    }

    async fn dispatch_json(
        &self,
        action_name: &str,
        input: Value,
    ) -> Result<(Receiver<Value>, Pin<Box<dyn Future<Output = ()> + Send>>)> {
        // Get typed event receiver
        let event_rx = self.dispatch_json(action_name, input).await?;

        // Create channel for JSON events
        let (tx, rx) = async_channel::unbounded();

        // Create the conversion future
        let converter = async move {
            while let Ok(event) = event_rx.recv().await {
                if let Ok(json) = serde_json::to_value(&event) {
                    let _ = tx.send(json).await;
                }
            }
        };

        Ok((rx, Box::pin(converter)))
    }

    async fn get_state(&self) -> Result<ServiceState> {
        // Default to Running state - services that need different behavior
        // should implement StatefulService
        Ok(ServiceState::Running)
    }

    async fn wait_for_state(&self, target: ServiceState, _timeout: Duration) -> Result<()> {
        // For now, just check if we're already in the target state
        let current = self.get_state().await?;
        if current == target {
            Ok(())
        } else {
            Err(Error::service_type(format!(
                "Service {} is in state {:?}, not {:?}",
                self.inner.name(),
                current,
                target
            )))
        }
    }
}

/// Wrapper for services that implement StatefulService and ServiceSetup
pub struct StatefulWrapper<S>
where
    S: Service + StatefulService + ServiceSetup,
    S::Action: JsonSchema,
    S::Event: JsonSchema,
{
    inner: S,
    action_schema: Value,
    event_schema: Value,
}

impl<S> StatefulWrapper<S>
where
    S: Service + StatefulService + ServiceSetup,
    S::Action: JsonSchema,
    S::Event: JsonSchema,
{
    pub fn new(service: S) -> Self {
        let action_schema = schemars::schema_for!(S::Action);
        let event_schema = schemars::schema_for!(S::Event);

        Self {
            inner: service,
            action_schema: serde_json::to_value(action_schema).unwrap(),
            event_schema: serde_json::to_value(event_schema).unwrap(),
        }
    }
}

#[async_trait]
impl<S> JsonService for StatefulWrapper<S>
where
    S: Service + StatefulService + ServiceSetup + 'static,
    S::Action: JsonSchema,
    S::Event: JsonSchema,
{
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn available_actions(&self) -> Vec<ActionDescriptor> {
        vec![ActionDescriptor {
            name: "default".to_string(),
            description: self.inner.description().to_string(),
            input_schema: self.action_schema.clone(),
            event_schema: self.event_schema.clone(),
        }]
    }

    async fn dispatch_json(
        &self,
        _action_name: &str,
        input: Value,
    ) -> Result<(Receiver<Value>, Pin<Box<dyn Future<Output = ()> + Send>>)> {
        // Deserialize JSON to typed action
        let action: S::Action = serde_json::from_value(input)
            .map_err(|e| Error::service_type(format!("Failed to deserialize action: {}", e)))?;

        // Execute the typed action
        let event_rx = self.inner.dispatch_action(action).await?;

        // Create channel for JSON events
        let (tx, rx) = async_channel::unbounded();

        // Create the conversion future
        let converter = async move {
            while let Ok(event) = event_rx.recv().await {
                if let Ok(json) = serde_json::to_value(&event) {
                    let _ = tx.send(json).await;
                }
            }
        };

        Ok((rx, Box::pin(converter)))
    }

    async fn get_state(&self) -> Result<ServiceState> {
        self.inner.get_state().await
    }

    async fn wait_for_state(&self, target: ServiceState, timeout: Duration) -> Result<()> {
        self.inner.wait_for_state(target, timeout).await
    }

    fn has_setup(&self) -> bool {
        true
    }

    async fn perform_setup(&self) -> Result<()> {
        self.inner.perform_setup().await
    }

    async fn is_setup_complete(&self) -> Result<bool> {
        self.inner.is_setup_complete().await
    }
}

/// Stack of services available in a daemon
///
/// The ServiceStack manages a collection of services that can perform actions.
/// Services are stored as trait objects to allow different service types.
pub struct ServiceStack {
    services: HashMap<String, Box<dyn JsonService>>,
    service_types: HashSet<String>,
}

impl ServiceStack {
    /// Create a new empty service stack
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
            service_types: HashSet::new(),
        }
    }

    /// Register a service in the stack
    ///
    /// The service must implement JsonSchema for its Action and Event types.
    pub fn register<S>(&mut self, instance_name: String, service: S) -> Result<()>
    where
        S: Service + 'static,
        S::Action: JsonSchema,
        S::Event: JsonSchema,
    {
        if self.services.contains_key(&instance_name) {
            return Err(Error::service_type(format!(
                "Service instance '{}' already registered",
                instance_name
            )));
        }

        // Track the service type
        self.service_types.insert(S::service_type().to_string());

        let wrapper = ServiceWrapper::new(service);
        self.services.insert(instance_name, Box::new(wrapper));
        Ok(())
    }

    /// Register a stateful service with setup support
    pub fn register_stateful<S>(&mut self, instance_name: String, service: S) -> Result<()>
    where
        S: Service + StatefulService + ServiceSetup + 'static,
        S::Action: JsonSchema,
        S::Event: JsonSchema,
    {
        if self.services.contains_key(&instance_name) {
            return Err(Error::service_type(format!(
                "Service instance '{}' already registered",
                instance_name
            )));
        }

        // Track the service type
        self.service_types.insert(S::service_type().to_string());

        let wrapper = StatefulWrapper::new(service);
        self.services.insert(instance_name, Box::new(wrapper));
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
    pub async fn dispatch<Sp: Spawner>(
        &self,
        instance_name: &str,
        action_name: &str,
        input: Value,
        spawner: &Sp,
    ) -> Result<Receiver<Value>> {
        let service = self.get(instance_name).ok_or_else(|| {
            Error::service_type(format!("Service instance '{}' not found", instance_name))
        })?;

        let (rx, converter) = service.dispatch_json(action_name, input).await?;

        // Spawn the converter
        spawner.spawn(converter);

        Ok(rx)
    }
}

impl Default for ServiceStack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_runtime_compat::smol::SmolSpawner;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    struct TestAction {
        message: String,
    }

    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    struct TestEvent {
        response: String,
    }

    struct TestService;

    #[async_trait]
    impl Service for TestService {
        type Action = TestAction;
        type Event = TestEvent;

        fn service_type() -> &'static str {
            "test-service"
        }

        fn name(&self) -> &str {
            "test-service"
        }

        fn description(&self) -> &str {
            "A test service"
        }

        async fn dispatch_action(&self, action: Self::Action) -> Result<Receiver<Self::Event>> {
            let (tx, rx) = async_channel::bounded(1);
            tx.send(TestEvent {
                response: format!("Echo: {}", action.message),
            })
            .await
            .unwrap();
            Ok(rx)
        }
    }

    #[test]
    fn test_service_stack_registration() {
        let mut stack = ServiceStack::new();
        let service = TestService;

        // Register service
        stack.register("test-1".to_string(), service).unwrap();

        // Check it's registered
        assert!(stack.get("test-1").is_some());
        assert_eq!(stack.list().len(), 1);

        // Try to register with same name (should fail)
        let result = stack.register("test-1".to_string(), TestService);
        assert!(result.is_err());
    }

    #[smol_potat::test]
    async fn test_action_dispatch() {
        let mut stack = ServiceStack::new();
        stack.register("test-1".to_string(), TestService).unwrap();

        let input = serde_json::json!({
            "message": "Hello"
        });

        let spawner = SmolSpawner;
        let rx = stack
            .dispatch("test-1", "default", input, &spawner)
            .await
            .unwrap();
        let event = rx.recv().await.unwrap();

        assert_eq!(event["response"], "Echo: Hello");
    }
}
