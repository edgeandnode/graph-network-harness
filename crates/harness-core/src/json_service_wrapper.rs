//! JSON service wrapper for services with json_actions
//!
//! This module provides a way to wrap services for JSON dispatch.
//! Services using the #[json_actions] macro will have a dispatch_json_action method generated.

use async_trait::async_trait;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use async_channel::Receiver;

use crate::action::{JsonActionRegistry, ServiceJsonActions};
use crate::service::{ActionDescriptor, JsonService, Service, ServiceState};
use crate::Error;
use std::result::Result;

/// Type for JSON dispatch functions
/// Services with #[json_actions] will generate a method that matches this signature
pub type JsonDispatchFn = fn(&dyn std::any::Any, &str, Value) -> Pin<Box<dyn Future<Output = Result<Value, Error>> + Send>>;

/// Wrapper that makes any service with json_actions into a JsonService
/// This requires providing a dispatch function since we can't access the generated method generically
pub struct JsonServiceWrapper<S> {
    service: Arc<S>,
    registry: JsonActionRegistry,
    dispatch_fn: JsonDispatchFn,
}

impl<S> JsonServiceWrapper<S>
where
    S: Service + ServiceJsonActions + 'static,
{
    /// Create a new JSON service wrapper with a dispatch function
    /// The dispatch_fn should call the service's dispatch_json_action method
    pub fn new_with_dispatch(service: S, dispatch_fn: JsonDispatchFn) -> Result<Self, Error> {
        let service = Arc::new(service);
        let mut registry = JsonActionRegistry::new();
        
        // Register the service's actions
        S::register_actions(&mut registry)?;
        
        Ok(Self {
            service,
            registry,
            dispatch_fn,
        })
    }
    
    /// Get the inner service
    pub fn inner(&self) -> &S {
        &self.service
    }
}

#[async_trait]
impl<S> JsonService for JsonServiceWrapper<S>
where
    S: Service + ServiceJsonActions + Send + Sync + 'static,
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
        
        // Call the dispatch function
        let service_any = &*self.service as &dyn std::any::Any;
        let future = (self.dispatch_fn)(service_any, action_name, input);
        
        // Create the async task that awaits the result and sends it
        let dispatcher = async move {
            match future.await {
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
}

/// Helper macro to create a dispatch function for a service
/// Usage: `create_json_dispatch!(ServiceType)`
#[macro_export]
macro_rules! create_json_dispatch {
    ($service_type:ty) => {
        |service_any: &dyn std::any::Any, action_name: &str, input: serde_json::Value| -> std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<serde_json::Value, harness_core::Error>> + Send>> {
            let service = service_any.downcast_ref::<$service_type>()
                .expect("Service type mismatch in dispatch");
            let action_name = action_name.to_string();
            let service = service.clone(); // Assuming the service is cloneable or use Arc
            
            Box::pin(async move {
                service.dispatch_json_action(&action_name, input).await
            })
        }
    };
}