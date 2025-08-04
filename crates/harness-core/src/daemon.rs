//! Core daemon abstractions and base implementation
//!
//! This module provides the `Daemon` trait and `BaseDaemon` implementation
//! that serves as the foundation for domain-specific daemons.

use async_trait::async_trait;
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;

use crate::action::{Action, ActionRegistry};
use crate::service::ServiceStack;
use crate::task::TaskStack;
use crate::{Error, Registry, Result, ServiceManager};

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

    /// Task stack for deployment tasks
    task_stack: TaskStack,

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

    /// Get the task stack
    pub fn task_stack(&self) -> &TaskStack {
        &self.task_stack
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
        self.running
            .store(true, std::sync::atomic::Ordering::SeqCst);

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
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);

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
    task_stack: TaskStack,
    config: Option<Value>,
    #[cfg(test)]
    test_mode: bool,
}

impl DaemonBuilder {
    /// Create a new daemon builder with defaults
    pub fn new() -> Self {
        Self {
            endpoint: "127.0.0.1:9443".parse().unwrap(),
            registry_path: None,
            action_registry: ActionRegistry::new(),
            service_stack: ServiceStack::new(),
            task_stack: TaskStack::new(),
            config: None,
            #[cfg(test)]
            test_mode: false,
        }
    }

    /// Enable test mode (uses temporary directories)
    #[cfg(test)]
    pub fn with_test_mode(mut self) -> Self {
        self.test_mode = true;
        self
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

    /// Get a mutable reference to the task stack for registration
    pub fn task_stack_mut(&mut self) -> &mut TaskStack {
        &mut self.task_stack
    }

    /// Set configuration for validation
    pub fn with_config(mut self, config: Value) -> Self {
        self.config = Some(config);
        self
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

        // Validate configuration if provided
        if let Some(config) = &self.config {
            self.validate_config(config)?;
        }

        // Create service manager
        #[cfg(test)]
        let service_manager = if self.test_mode {
            ServiceManager::new_for_tests()
                .await
                .map_err(Error::ServiceOrchestration)?
        } else {
            ServiceManager::new()
                .await
                .map_err(Error::ServiceOrchestration)?
        };

        #[cfg(not(test))]
        let service_manager = ServiceManager::new()
            .await
            .map_err(Error::ServiceOrchestration)?;

        // Create service registry
        let service_registry = if let Some(path) = &self.registry_path {
            info!("Creating registry with persistence at {}", path);
            Registry::with_persistence(path).await
        } else {
            Registry::new().await
        };

        Ok(BaseDaemon {
            service_manager,
            service_registry,
            action_registry: self.action_registry,
            service_stack: self.service_stack,
            task_stack: self.task_stack,
            endpoint: self.endpoint,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    /// Validate configuration against registered services and tasks
    fn validate_config(&self, config: &Value) -> Result<()> {
        let services = config.get("services").and_then(|s| s.as_object());
        
        // Validate services
        if let Some(service_map) = services {
            info!("Validating {} service configurations", service_map.len());
            
            for (name, service_config) in service_map {
                // Validate service_type exists
                if let Some(service_type) = service_config.get("service_type").and_then(|s| s.as_str()) {
                    // Get registered service types from the service stack
                    let registered_types = self.service_stack.list_types();
                    
                    if !registered_types.contains(&service_type) {
                        return Err(Error::validation(format!(
                            "Service '{}' references unknown service_type '{}'. Available types: {:?}",
                            name, service_type, registered_types
                        )));
                    }
                }
                
                // Validate dependencies
                if let Some(deps) = service_config.get("dependencies").and_then(|d| d.as_array()) {
                    for dep in deps {
                        self.validate_dependency(dep, service_map, config)?;
                    }
                }
            }
        }

        // Validate tasks
        if let Some(tasks) = config.get("tasks").and_then(|t| t.as_object()) {
            info!("Validating {} task configurations", tasks.len());
            
            for (name, task_config) in tasks {
                // Validate task_type exists
                if let Some(task_type) = task_config.get("task_type").and_then(|t| t.as_str()) {
                    let registered_types = self.task_stack.list_types();
                    
                    if !registered_types.contains(&task_type) {
                        return Err(Error::validation(format!(
                            "Task '{}' references unknown task_type '{}'. Available types: {:?}",
                            name, task_type, registered_types
                        )));
                    }
                }
                
                // Validate dependencies
                if let Some(deps) = task_config.get("dependencies").and_then(|d| d.as_array()) {
                    let empty_services = serde_json::Map::new();
                    let service_map = services.unwrap_or(&empty_services);
                    for dep in deps {
                        self.validate_dependency(dep, service_map, config)?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Validate a single dependency
    fn validate_dependency(
        &self,
        dep: &Value,
        services: &serde_json::Map<String, Value>,
        config: &Value,
    ) -> Result<()> {
        // Check if it's a service dependency
        if let Some(service_name) = dep.get("service").and_then(|s| s.as_str()) {
            if !services.contains_key(service_name) {
                return Err(Error::validation(format!(
                    "Dependency references unknown service '{}'",
                    service_name
                )));
            }
        }
        // Check if it's a task dependency
        else if let Some(task_name) = dep.get("task").and_then(|t| t.as_str()) {
            if let Some(tasks) = config.get("tasks").and_then(|t| t.as_object()) {
                if !tasks.contains_key(task_name) {
                    return Err(Error::validation(format!(
                        "Dependency references unknown task '{}'",
                        task_name
                    )));
                }
            } else {
                return Err(Error::validation(format!(
                    "Dependency references task '{}' but no tasks are defined",
                    task_name
                )));
            }
        }
        
        Ok(())
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
    use crate::service::Service;
    use crate::task::DeploymentTask;
    use serde_json::json;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    // Test service for validation
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    struct TestAction;

    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    struct TestEvent;

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
            "Test service"
        }

        async fn dispatch_action(&self, _action: Self::Action) -> Result<async_channel::Receiver<Self::Event>> {
            let (tx, rx) = async_channel::bounded(1);
            tx.send(TestEvent).await.unwrap();
            Ok(rx)
        }
    }

    // Test task for validation
    struct TestTask;

    #[async_trait]
    impl DeploymentTask for TestTask {
        type Action = TestAction;
        type Event = TestEvent;

        fn task_type() -> &'static str {
            "test-task"
        }

        fn name(&self) -> &str {
            "test-task"
        }

        fn description(&self) -> &str {
            "Test task"
        }

        async fn is_completed(&self) -> Result<bool> {
            Ok(true)
        }

        async fn execute(&self, _action: Self::Action) -> Result<async_channel::Receiver<Self::Event>> {
            let (tx, rx) = async_channel::bounded(1);
            tx.send(TestEvent).await.unwrap();
            Ok(rx)
        }
    }

    #[smol_potat::test]
    async fn test_daemon_builder() {
        let daemon = BaseDaemon::builder()
            .with_test_mode()
            .with_endpoint("127.0.0.1:8080".parse().unwrap())
            .register_action("test", "Test action", |params| async move {
                Ok::<_, Error>(json!({ "echo": params }))
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
        let daemon = BaseDaemon::builder()
            .with_test_mode()
            .register_action("echo", "Echo the input", |params| async move {
                Ok::<_, Error>(json!({ "result": params }))
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

    #[smol_potat::test]
    async fn test_validation_missing_service_type() {
        let config = json!({
            "services": {
                "test-svc": {
                    "service_type": "unknown-type",
                    "dependencies": []
                }
            }
        });

        let result = BaseDaemon::builder()
            .with_test_mode()
            .with_config(config)
            .build()
            .await;

        assert!(result.is_err());
        if let Err(err) = result {
            assert!(err.to_string().contains("unknown service_type 'unknown-type'"));
        }
    }

    #[smol_potat::test]
    async fn test_validation_with_valid_service() {
        let mut builder = BaseDaemon::builder();
        builder.service_stack_mut().register("test-instance".to_string(), TestService).unwrap();

        let config = json!({
            "services": {
                "test-svc": {
                    "service_type": "test-service",
                    "dependencies": []
                }
            }
        });

        let daemon = builder
            .with_test_mode()
            .with_config(config)
            .build()
            .await
            .unwrap();

        assert!(daemon.service_stack().get("test-instance").is_some());
    }

    #[smol_potat::test]
    async fn test_validation_missing_task_type() {
        let config = json!({
            "tasks": {
                "test-task": {
                    "task_type": "unknown-task",
                    "dependencies": []
                }
            }
        });

        let result = BaseDaemon::builder()
            .with_test_mode()
            .with_config(config)
            .build()
            .await;

        assert!(result.is_err());
        if let Err(err) = result {
            assert!(err.to_string().contains("unknown task_type 'unknown-task'"));
        }
    }

    #[smol_potat::test]
    async fn test_validation_with_valid_task() {
        let mut builder = BaseDaemon::builder();
        builder.task_stack_mut().register("test-task-instance".to_string(), TestTask).unwrap();

        let config = json!({
            "tasks": {
                "test-task-instance": {
                    "task_type": "test-task",
                    "dependencies": []
                }
            }
        });

        let daemon = builder
            .with_test_mode()
            .with_config(config)
            .build()
            .await
            .unwrap();

        assert!(daemon.task_stack().list_types().contains(&"test-task"));
    }

    #[smol_potat::test]
    async fn test_validation_missing_service_dependency() {
        let mut builder = BaseDaemon::builder();
        builder.service_stack_mut().register("test-instance".to_string(), TestService).unwrap();

        let config = json!({
            "services": {
                "test-svc": {
                    "service_type": "test-service",
                    "dependencies": [
                        { "service": "missing-service" }
                    ]
                }
            }
        });

        let result = builder
            .with_test_mode()
            .with_config(config)
            .build()
            .await;

        assert!(result.is_err());
        if let Err(err) = result {
            assert!(err.to_string().contains("unknown service 'missing-service'"));
        }
    }

    #[smol_potat::test]
    async fn test_validation_missing_task_dependency() {
        let mut builder = BaseDaemon::builder();
        builder.service_stack_mut().register("test-instance".to_string(), TestService).unwrap();

        let config = json!({
            "services": {
                "test-svc": {
                    "service_type": "test-service",
                    "dependencies": [
                        { "task": "missing-task" }
                    ]
                }
            }
        });

        let result = builder
            .with_test_mode()
            .with_config(config)
            .build()
            .await;

        assert!(result.is_err());
        if let Err(err) = result {
            assert!(err.to_string().contains("no tasks are defined"));
        }
    }

    #[smol_potat::test]
    async fn test_validation_valid_mixed_dependencies() {
        let mut builder = BaseDaemon::builder();
        builder.service_stack_mut().register("svc1".to_string(), TestService).unwrap();
        builder.service_stack_mut().register("svc2".to_string(), TestService).unwrap();
        builder.task_stack_mut().register("task1".to_string(), TestTask).unwrap();

        let config = json!({
            "services": {
                "svc1": {
                    "service_type": "test-service",
                    "dependencies": []
                },
                "svc2": {
                    "service_type": "test-service",
                    "dependencies": [
                        { "service": "svc1" },
                        { "task": "task1" }
                    ]
                }
            },
            "tasks": {
                "task1": {
                    "task_type": "test-task",
                    "dependencies": []
                }
            }
        });

        let daemon = builder
            .with_test_mode()
            .with_config(config)
            .build()
            .await
            .unwrap();

        assert!(daemon.service_stack().get("svc1").is_some());
        assert!(daemon.service_stack().get("svc2").is_some());
    }

    #[smol_potat::test]
    async fn test_validation_task_with_service_dependency() {
        let mut builder = BaseDaemon::builder();
        builder.service_stack_mut().register("test-svc".to_string(), TestService).unwrap();
        builder.task_stack_mut().register("task1".to_string(), TestTask).unwrap();

        let config = json!({
            "services": {
                "test-svc": {
                    "service_type": "test-service",
                    "dependencies": []
                }
            },
            "tasks": {
                "task1": {
                    "task_type": "test-task",
                    "dependencies": [
                        { "service": "test-svc" }
                    ]
                }
            }
        });

        let daemon = builder
            .with_test_mode()
            .with_config(config)
            .build()
            .await
            .unwrap();

        assert!(daemon.task_stack().list_types().contains(&"test-task"));
    }

    // NEW: Integration tests for mixed dependencies
    
    #[smol_potat::test]
    async fn test_circular_dependency_detection() {
        let mut builder = BaseDaemon::builder();
        builder.service_stack_mut().register("svc1".to_string(), TestService).unwrap();
        builder.service_stack_mut().register("svc2".to_string(), TestService).unwrap();

        // Service depends on itself indirectly through another service
        let config = json!({
            "services": {
                "svc1": {
                    "service_type": "test-service",
                    "dependencies": [
                        { "service": "svc2" }
                    ]
                },
                "svc2": {
                    "service_type": "test-service",
                    "dependencies": [
                        { "service": "svc1" }
                    ]
                }
            }
        });

        // This should pass validation as we don't detect circular dependencies here
        // That would be done at execution time by topological sort
        let daemon = builder
            .with_test_mode()
            .with_config(config)
            .build()
            .await
            .unwrap();

        assert!(daemon.service_stack().get("svc1").is_some());
    }

    #[smol_potat::test]
    async fn test_complex_dependency_chain() {
        let mut builder = BaseDaemon::builder();
        
        // Register multiple services
        builder.service_stack_mut().register("db".to_string(), TestService).unwrap();
        builder.service_stack_mut().register("cache".to_string(), TestService).unwrap();
        builder.service_stack_mut().register("api".to_string(), TestService).unwrap();
        builder.service_stack_mut().register("web".to_string(), TestService).unwrap();
        
        // Register tasks
        builder.task_stack_mut().register("db-migrate".to_string(), TestTask).unwrap();
        builder.task_stack_mut().register("cache-warm".to_string(), TestTask).unwrap();

        let config = json!({
            "services": {
                "db": {
                    "service_type": "test-service",
                    "dependencies": []
                },
                "cache": {
                    "service_type": "test-service",
                    "dependencies": [
                        { "service": "db" },
                        { "task": "db-migrate" }
                    ]
                },
                "api": {
                    "service_type": "test-service",
                    "dependencies": [
                        { "service": "db" },
                        { "service": "cache" },
                        { "task": "cache-warm" }
                    ]
                },
                "web": {
                    "service_type": "test-service",
                    "dependencies": [
                        { "service": "api" }
                    ]
                }
            },
            "tasks": {
                "db-migrate": {
                    "task_type": "test-task",
                    "dependencies": [
                        { "service": "db" }
                    ]
                },
                "cache-warm": {
                    "task_type": "test-task",
                    "dependencies": [
                        { "service": "cache" }
                    ]
                }
            }
        });

        let daemon = builder
            .with_test_mode()
            .with_config(config)
            .build()
            .await
            .unwrap();

        assert!(daemon.service_stack().get("db").is_some());
        assert!(daemon.service_stack().get("web").is_some());
        assert_eq!(daemon.task_stack().list_types().len(), 1);
    }

    #[smol_potat::test]
    async fn test_task_depending_on_task() {
        let mut builder = BaseDaemon::builder();
        builder.task_stack_mut().register("task1".to_string(), TestTask).unwrap();
        builder.task_stack_mut().register("task2".to_string(), TestTask).unwrap();

        let config = json!({
            "tasks": {
                "task1": {
                    "task_type": "test-task",
                    "dependencies": []
                },
                "task2": {
                    "task_type": "test-task",
                    "dependencies": [
                        { "task": "task1" }
                    ]
                }
            }
        });

        let daemon = builder
            .with_test_mode()
            .with_config(config)
            .build()
            .await
            .unwrap();

        assert_eq!(daemon.task_stack().list_types().len(), 1); // Only one type registered
    }
}