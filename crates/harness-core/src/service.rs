//! Service stack system for defining services with actions
//!
//! This module provides traits and utilities for defining services that can
//! perform actions. Services are strongly typed with their action and event
//! types, while a wrapper layer handles JSON serialization for wire protocol.

use async_trait::async_trait;
use async_channel::Receiver;
use serde::de::DeserializeOwned;
use serde::{Serialize, Deserialize};
use serde_json::Value;
use schemars::JsonSchema;
use std::collections::HashMap;

use crate::{Error, Result};

/// Trait for services that can perform actions
///
/// Services are strongly typed with their Action and Event types.
/// This trait focuses on domain logic without JSON concerns.
#[async_trait]
pub trait Service: Send + Sync + 'static {
    /// The input type for actions this service can perform
    type Action: DeserializeOwned + Send;
    
    /// The event type emitted during action execution
    type Event: Serialize + Send;
    
    /// Get the service name
    fn name(&self) -> &str;
    
    /// Get the service description
    fn description(&self) -> &str;
    
    /// Execute an action, returning a stream of events
    async fn dispatch_action(&self, action: Self::Action) -> Result<Receiver<Self::Event>>;
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
    
    /// Dispatch an action using JSON input
    pub async fn dispatch_json(&self, _action_name: &str, input: Value) -> Result<Receiver<Value>> {
        // Deserialize JSON to typed action
        let action: S::Action = serde_json::from_value(input)
            .map_err(|e| Error::service_type(format!("Failed to deserialize action: {}", e)))?;
        
        // Execute the typed action
        let event_rx = self.inner.dispatch_action(action).await?;
        
        // Create channel for JSON events
        let (tx, rx) = async_channel::unbounded();
        
        // Spawn task to convert events to JSON
        // Note: In production, this would use the runtime's spawn mechanism
        std::thread::spawn(move || {
            futures::executor::block_on(async {
                while let Ok(event) = event_rx.recv().await {
                    if let Ok(json) = serde_json::to_value(&event) {
                        let _ = tx.send(json).await;
                    }
                }
            });
        });
        
        Ok(rx)
    }
}

/// Trait for services that work with JSON (used for dynamic dispatch)
#[async_trait]
pub trait JsonService: Send + Sync {
    /// Get the service name
    fn name(&self) -> &str;
    
    /// Get the service description
    fn description(&self) -> &str;
    
    /// Get available actions
    fn available_actions(&self) -> Vec<ActionDescriptor>;
    
    /// Dispatch an action using JSON
    async fn dispatch_json(&self, action_name: &str, input: Value) -> Result<Receiver<Value>>;
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
    
    async fn dispatch_json(&self, action_name: &str, input: Value) -> Result<Receiver<Value>> {
        self.dispatch_json(action_name, input).await
    }
}

/// Stack of services available in a daemon
///
/// The ServiceStack manages a collection of services that can perform actions.
/// Services are stored as trait objects to allow different service types.
pub struct ServiceStack {
    services: HashMap<String, Box<dyn JsonService>>,
}

impl ServiceStack {
    /// Create a new empty service stack
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
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
        
        let wrapper = ServiceWrapper::new(service);
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
    
    /// Dispatch an action to a specific service instance
    pub async fn dispatch(
        &self,
        instance_name: &str,
        action_name: &str,
        input: Value,
    ) -> Result<Receiver<Value>> {
        let service = self
            .get(instance_name)
            .ok_or_else(|| Error::service_type(format!("Service instance '{}' not found", instance_name)))?;
        
        service.dispatch_json(action_name, input).await
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
    use serde::{Deserialize, Serialize};
    use schemars::JsonSchema;
    
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
            
            let rx = stack.dispatch("test-1", "default", input).await.unwrap();
            let event = rx.recv().await.unwrap();
            
            assert_eq!(event["response"], "Echo: Hello");
    }
}