//! Core daemon abstractions and base implementation
//!
//! This module provides the `Daemon` trait and `BaseDaemon` implementation
//! that serves as the foundation for domain-specific daemons.

use async_trait::async_trait;
use serde_json::Value;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};

use crate::action::{Action, ActionRegistry};
use crate::service::ServiceStack;
use crate::{Error, Result, ServiceManager, Registry};

/// Core daemon trait that all harness daemons must implement
#[async_trait]
pub trait Daemon: Action + Send + Sync {
    /// Start the daemon
    async fn start(&self) -> Result<()>;
    
    /// Stop the daemon gracefully
    async fn stop(&self) -> Result<()>;
    
    /// Get the WebSocket endpoint this daemon listens on
    fn endpoint(&self) -> SocketAddr;
    
    /// Get the service manager
    fn service_manager(&self) -> &ServiceManager;
    
    /// Get the service registry
    fn service_registry(&self) -> &Registry;
}

/// Base daemon implementation that provides core functionality
pub struct BaseDaemon {
    /// Service manager for orchestrating services
    service_manager: ServiceManager,
    
    /// Service registry for discovery
    service_registry: Registry,
    
    /// Action registry for custom functionality
    action_registry: ActionRegistry,
    
    /// Service stack for actionable services
    service_stack: ServiceStack,
    
    /// WebSocket server address
    endpoint: SocketAddr,
    
    /// Whether the daemon is running
    running: Arc<std::sync::atomic::AtomicBool>,
}

impl BaseDaemon {
    /// Create a new daemon builder
    pub fn builder() -> DaemonBuilder {
        DaemonBuilder::new()
    }
    
    /// Get the service manager
    pub fn service_manager(&self) -> &ServiceManager {
        &self.service_manager
    }
    
    /// Get the service registry
    pub fn service_registry(&self) -> &Registry {
        &self.service_registry
    }
    
    /// Get the endpoint
    pub fn endpoint(&self) -> SocketAddr {
        self.endpoint
    }
    
    /// Get the service stack
    pub fn service_stack(&self) -> &ServiceStack {
        &self.service_stack
    }
}

#[async_trait]
impl Action for BaseDaemon {
    fn actions(&self) -> &ActionRegistry {
        &self.action_registry
    }
    
    fn actions_mut(&mut self) -> &mut ActionRegistry {
        &mut self.action_registry
    }
}

#[async_trait]
impl Daemon for BaseDaemon {
    async fn start(&self) -> Result<()> {
        info!("Starting base daemon on {}", self.endpoint);
        
        // Mark as running
        self.running.store(true, std::sync::atomic::Ordering::SeqCst);
        
        // TODO: Start WebSocket server
        // This would start the WebSocket server that handles:
        // - Service management requests
        // - Action discovery and invocation
        // - Event streaming
        
        info!("Base daemon started successfully");
        Ok(())
    }
    
    async fn stop(&self) -> Result<()> {
        info!("Stopping base daemon");
        
        // Mark as stopped
        self.running.store(false, std::sync::atomic::Ordering::SeqCst);
        
        // TODO: Stop WebSocket server and cleanup
        
        info!("Base daemon stopped");
        Ok(())
    }
    
    fn endpoint(&self) -> SocketAddr {
        self.endpoint
    }
    
    fn service_manager(&self) -> &ServiceManager {
        &self.service_manager
    }
    
    fn service_registry(&self) -> &Registry {
        &self.service_registry
    }
}

/// Builder for creating daemon instances
pub struct DaemonBuilder {
    endpoint: SocketAddr,
    registry_path: Option<String>,
    action_registry: ActionRegistry,
    service_stack: ServiceStack,
}

impl DaemonBuilder {
    /// Create a new daemon builder with defaults
    pub fn new() -> Self {
        Self {
            endpoint: "127.0.0.1:9443".parse().unwrap(),
            registry_path: None,
            action_registry: ActionRegistry::new(),
            service_stack: ServiceStack::new(),
        }
    }
    
    /// Set the WebSocket endpoint
    pub fn with_endpoint(mut self, endpoint: SocketAddr) -> Self {
        self.endpoint = endpoint;
        self
    }
    
    /// Set the registry persistence path
    pub fn with_registry_path(mut self, path: impl Into<String>) -> Self {
        self.registry_path = Some(path.into());
        self
    }
    
    /// Get a mutable reference to the service stack for registration
    pub fn service_stack_mut(&mut self) -> &mut ServiceStack {
        &mut self.service_stack
    }
    
    /// Register an action
    pub fn register_action<F, Fut>(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        action: F,
    ) -> Result<Self>
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<Value>> + Send + 'static,
    {
        self.action_registry
            .register_simple(name, description, action)?;
        Ok(self)
    }
    
    
    /// Build the daemon
    pub async fn build(self) -> Result<BaseDaemon> {
        info!("Building daemon with endpoint {}", self.endpoint);
        
        // Create service manager
        let service_manager = ServiceManager::new().await.map_err(Error::ServiceOrchestration)?;
        
        // Create service registry
        let service_registry = if let Some(path) = &self.registry_path {
            // Try to load existing registry or create new one
            if Path::new(path).exists() {
                match Registry::load(path).await {
                    Ok(registry) => {
                        info!("Loaded existing registry from {}", path);
                        registry
                    }
                    Err(e) => {
                        warn!("Failed to load registry: {}. Creating new one.", e);
                        Registry::with_persistence(path)
                    }
                }
            } else {
                info!("Creating new registry at {}", path);
                Registry::with_persistence(path)
            }
        } else {
            Registry::new()
        };
        
        Ok(BaseDaemon {
            service_manager,
            service_registry,
            action_registry: self.action_registry,
            service_stack: self.service_stack,
            endpoint: self.endpoint,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }
}

impl Default for DaemonBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[smol_potat::test]
    async fn test_daemon_builder() {
            let daemon = BaseDaemon::builder()
                .with_endpoint("127.0.0.1:8080".parse().unwrap())
                .register_action("test", "Test action", |params| async move {
                    Ok(json!({ "echo": params }))
                })
                .unwrap()
                .build()
                .await
                .unwrap();
            
            assert_eq!(daemon.endpoint().port(), 8080);
            assert!(daemon.actions().has_action("test"));
    }
    
    #[smol_potat::test]
    async fn test_action_invocation() {
            let mut daemon = BaseDaemon::builder()
                .register_action("echo", "Echo the input", |params| async move {
                    Ok(json!({ "result": params }))
                })
                .unwrap()
                .build()
                .await
                .unwrap();
            
            let result = daemon
                .invoke_action("echo", json!({ "message": "hello" }))
                .await
                .unwrap();
            
            assert_eq!(result, json!({ "result": { "message": "hello" } }));
    }
}