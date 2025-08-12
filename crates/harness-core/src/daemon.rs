//! Core daemon abstractions and base implementation
//!
//! This module provides the `Daemon` trait and `BaseDaemon` implementation
//! that serves as the foundation for domain-specific daemons.

use async_runtime_compat::prelude::*;
use async_runtime_compat::smol::SmolSpawner;
use async_trait::async_trait;
use serde_json::{Value, json};
use service_orchestration::{
    DependencyGraph, DependencyNode, ServiceConfig, ServiceStatus, StackConfig,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

use crate::action::{Action, ActionRegistry};
use crate::config_traits::{ServiceFromConfig, TaskFromConfig};
use crate::service::{Service, JsonServiceRegistry};
use crate::task::{DeploymentTask, JsonTaskRegistry};
use crate::{Error, Registry, ServiceManager};
use service_orchestration::TaskConfig;
use std::result::Result;

/// Core daemon trait that all harness daemons must implement
#[async_trait]
pub trait Daemon: Action + Send + Sync {
    /// Start the daemon
    async fn start(&self) -> Result<(), Error>;

    /// Stop the daemon gracefully
    async fn stop(&self) -> Result<(), Error>;

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

    /// Registry of JSON-wrapped services
    json_service_registry: JsonServiceRegistry,

    /// Registry of JSON-wrapped tasks
    json_task_registry: JsonTaskRegistry,

    /// WebSocket server address
    endpoint: SocketAddr,

    /// Whether the daemon is running
    running: Arc<std::sync::atomic::AtomicBool>,

    /// Stack configuration if provided
    stack_config: Option<StackConfig>,
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

    /// Get the JSON service registry
    pub fn json_service_registry(&self) -> &JsonServiceRegistry {
        &self.json_service_registry
    }

    /// Get the JSON task registry
    pub fn json_task_registry(&self) -> &JsonTaskRegistry {
        &self.json_task_registry
    }

    /// Launch all services in the stack in dependency order
    pub async fn launch_stack(&self) -> Result<(), Error> {
        let config = self
            .stack_config
            .as_ref()
            .ok_or_else(|| Error::daemon("No stack configuration provided"))?;

        info!("Launching stack: {}", config.name);

        // Build dependency graph from config
        let graph = DependencyGraph::from_stack_config(config);

        // Get topological sort for startup order
        let start_order = graph
            .topological_sort()
            .map_err(|e| Error::daemon(format!("Failed to resolve dependencies: {e}")))?;

        info!("Starting services in dependency order: {:?}", start_order);

        // Start each service in order
        for node in start_order {
            match node {
                DependencyNode::Service(name) => {
                    // Get the service config
                    let service_instance = config.services.get(&name).ok_or_else(|| {
                        Error::daemon(format!("Service {name} not found in config"))
                    })?;

                    info!("Starting service: {}", name);

                    // Convert to ServiceConfig for ServiceManager
                    let service_config = ServiceConfig {
                        name: name.clone(),
                        target: service_instance.orchestration.target.clone(),
                        dependencies: service_instance.orchestration.dependencies.clone(),
                        health_check: service_instance.orchestration.health_check.clone(),
                    };

                    // Start the service via ServiceManager
                    let (_event_receiver, _running_service) = self
                        .service_manager
                        .launch_service(&name, service_config, &SmolSpawner)
                        .await
                        .map_err(|e| {
                            Error::daemon(format!("Failed to start service {name}: {e}"))
                        })?;

                    // Wait for health check if configured
                    if let Some(health_check) = &service_instance.orchestration.health_check {
                        let timeout = Duration::from_secs(health_check.timeout);
                        self.wait_for_service_health(&name, timeout).await?;
                    }

                    info!("Service {} started successfully", name);
                }
                DependencyNode::Task(name) => {
                    info!("Processing task: {}", name);

                    // Execute the task
                    self.execute_task(&name).await.map_err(|e| {
                        Error::daemon(format!("Failed to execute task {name}: {e}"))
                    })?;

                    info!("Task {} executed successfully", name);
                }
            }
        }

        info!("All services launched successfully");
        Ok(())
    }

    /// Execute a task
    async fn execute_task(&self, task_name: &str) -> Result<(), Error> {
        use async_runtime_compat::smol::SmolSpawner;

        info!("Executing task: {}", task_name);

        let config = self
            .stack_config
            .as_ref()
            .ok_or_else(|| Error::daemon("No stack configuration provided"))?;

        // Get the task config
        let task_config = config
            .tasks
            .get(task_name)
            .ok_or_else(|| Error::daemon(format!("Task {task_name} not found in config")))?;

        // Check if task is already completed by examining its state
        // Tasks define their own completion states (e.g., Completed, Failed)
        // We can't generically check this without knowing the specific state type

        // Check if we have a registered task for this task_type
        if self.json_task_registry.get(task_name).is_some() {
            // Execute the task using the JsonTaskRegistry which handles spawning
            info!("Executing registered task: {}", task_name);

            // Execute using JsonTaskRegistry's execute method which handles spawning
            let spawner = SmolSpawner;
            let state_rx = self.json_task_registry.execute(task_name, &spawner).await?;

            // Wait for task to complete by consuming all state updates
            while let Ok(state) = state_rx.recv().await {
                info!("Task {} state: {:?}", task_name, state);
            }

            info!("Task {} completed successfully", task_name);
        } else {
            // No registered task implementation, execute directly via command-executor
            info!(
                "Executing task {} via command executor (no registered implementation)",
                task_name
            );

            // For now, we'll just log that we would execute it
            // In a real implementation, we'd use command-executor to run the target
            warn!(
                "Direct task execution not yet implemented for task: {}",
                task_name
            );
        }

        Ok(())
    }

    /// Wait for a service to become healthy
    async fn wait_for_service_health(
        &self,
        service_name: &str,
        timeout: Duration,
    ) -> Result<(), Error> {
        info!(
            "Waiting for service {} to become healthy (timeout: {:?})",
            service_name, timeout
        );

        let start = std::time::Instant::now();

        loop {
            // Check service status
            match self.service_manager.get_service_status(service_name).await {
                Ok(ServiceStatus::Running) => {
                    info!("Service {} is running and healthy", service_name);
                    return Ok(());
                }
                Ok(status) => {
                    if start.elapsed() > timeout {
                        return Err(Error::daemon(format!(
                            "Service {service_name} failed to become healthy within timeout. Current status: {status:?}"
                        )));
                    }
                    // Continue waiting
                    sleep(Duration::from_secs(1)).await;
                }
                Err(e) => {
                    warn!("Error checking health for service {}: {}", service_name, e);
                    if start.elapsed() > timeout {
                        return Err(Error::daemon(format!(
                            "Service {service_name} health check timed out: {e}"
                        )));
                    }
                    sleep(Duration::from_secs(1)).await;
                }
            }
        }
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
    async fn start(&self) -> Result<(), Error> {
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

    async fn stop(&self) -> Result<(), Error> {
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
    state_dir: Option<std::path::PathBuf>,
    action_registry: ActionRegistry,
    json_service_registry: JsonServiceRegistry,
    json_task_registry: JsonTaskRegistry,
    config: Option<Value>,
    stack_config: Option<StackConfig>,
    #[cfg(test)]
    test_mode: bool,
}

impl DaemonBuilder {
    /// Create a new daemon builder with defaults
    pub fn new() -> Self {
        Self {
            endpoint: "127.0.0.1:9443".parse().unwrap(),
            state_dir: None,
            action_registry: ActionRegistry::new(),
            json_service_registry: JsonServiceRegistry::new(),
            json_task_registry: JsonTaskRegistry::new(),
            config: None,
            stack_config: None,
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

    /// Set the state directory
    pub fn with_state_dir(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.state_dir = Some(path.into());
        self
    }


    /// Set configuration for validation
    pub fn with_config(mut self, config: Value) -> Self {
        self.config = Some(config);
        self
    }

    /// Set typed stack configuration for orchestration
    pub fn with_stack_config(mut self, config: StackConfig) -> Self {
        self.stack_config = Some(config);
        self
    }

    /// Register a service with the JSON service registry
    pub fn register_service<S>(
        &mut self,
        instance_name: String,
        service: S,
    ) -> Result<&mut Self, Error>
    where
        S: Service + 'static,
        S::Action: schemars::JsonSchema,
        S::Event: schemars::JsonSchema,
    {
        // Log the service registration
        tracing::info!(
            "Registering service '{}' of type '{}'",
            instance_name,
            S::service_type()
        );

        // Register with the JSON service registry
        self.json_service_registry.register(instance_name, service)?;

        Ok(self)
    }

    /// Register a service from configuration using ServiceFromConfig trait
    pub fn register_service_from_config<S>(
        &mut self,
        instance_name: String,
        config: &ServiceConfig,
    ) -> Result<&mut Self, Error>
    where
        S: Service + ServiceFromConfig + 'static,
        S::Action: schemars::JsonSchema,
        S::Event: schemars::JsonSchema,
    {
        // Create the service from config
        let service = S::from_config(config)?;
        
        // Use the existing register_service method
        self.register_service(instance_name, service)
    }

    /// Register a task with the task stack
    pub fn register_task<T>(&mut self, task_name: String, task: T) -> Result<&mut Self, Error>
    where
        T: DeploymentTask + 'static,
        T::State: schemars::JsonSchema,
    {
        // Log the task registration
        tracing::info!("Registering task '{}'", task_name);

        self.json_task_registry.register(task_name, task)?;
        Ok(self)
    }

    /// Register a task from configuration using TaskFromConfig trait
    pub fn register_task_from_config<T>(
        &mut self,
        task_name: String,
        config: &TaskConfig,
    ) -> Result<&mut Self, Error>
    where
        T: DeploymentTask + TaskFromConfig + 'static,
        T::State: schemars::JsonSchema,
    {
        // Create the task from config
        let task = T::from_config(config)?;
        
        // Use the existing register_task method
        self.register_task(task_name, task)
    }

    /// Register an action
    pub fn register_action<F, Fut>(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        action: F,
    ) -> Result<Self, Error>
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<Value, Error>> + Send + 'static,
    {
        self.action_registry
            .register_simple(name, description, action)?;
        Ok(self)
    }

    /// Build the daemon
    pub async fn build(self) -> Result<BaseDaemon, Error> {
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
        } else if let Some(state_dir) = &self.state_dir {
            ServiceManager::with_state_dir(state_dir.clone())
                .await
                .map_err(Error::ServiceOrchestration)?
        } else {
            ServiceManager::new()
                .await
                .map_err(Error::ServiceOrchestration)?
        };

        #[cfg(not(test))]
        let service_manager = if let Some(state_dir) = &self.state_dir {
            ServiceManager::with_state_dir(state_dir.clone())
                .await
                .map_err(Error::ServiceOrchestration)?
        } else {
            ServiceManager::new()
                .await
                .map_err(Error::ServiceOrchestration)?
        };

        // Create service registry (always in-memory)
        let service_registry = Registry::new().await;

        Ok(BaseDaemon {
            service_manager,
            service_registry,
            action_registry: self.action_registry,
            json_service_registry: self.json_service_registry,
            json_task_registry: self.json_task_registry,
            endpoint: self.endpoint,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            stack_config: self.stack_config,
        })
    }

    /// Validate configuration against registered services and tasks
    fn validate_config(&self, config: &Value) -> Result<(), Error> {
        let services = config.get("services").and_then(|s| s.as_object());

        // Validate services
        if let Some(service_map) = services {
            info!("Validating {} service configurations", service_map.len());

            for (name, service_config) in service_map {
                // Validate service_type exists
                if let Some(service_type) =
                    service_config.get("service_type").and_then(|s| s.as_str())
                {
                    // Get registered service types from the service stack
                    let registered_types = self.json_service_registry.list_types();

                    if !registered_types.contains(&service_type) {
                        return Err(Error::validation(format!(
                            "Service '{name}' references unknown service_type '{service_type}'. Available types: {registered_types:?}"
                        )));
                    }
                }

                // Validate dependencies
                if let Some(deps) = service_config
                    .get("dependencies")
                    .and_then(|d| d.as_array())
                {
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
                    let registered_types = self.json_task_registry.list_types();

                    if !registered_types.contains(&task_type) {
                        return Err(Error::validation(format!(
                            "Task '{name}' references unknown task_type '{task_type}'. Available types: {registered_types:?}"
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
    ) -> Result<(), Error> {
        // Check if it's a service dependency
        if let Some(service_name) = dep.get("service").and_then(|s| s.as_str()) {
            if !services.contains_key(service_name) {
                return Err(Error::validation(format!(
                    "Dependency references unknown service '{service_name}'"
                )));
            }
        }
        // Check if it's a task dependency
        else if let Some(task_name) = dep.get("task").and_then(|t| t.as_str()) {
            if let Some(tasks) = config.get("tasks").and_then(|t| t.as_object()) {
                if !tasks.contains_key(task_name) {
                    return Err(Error::validation(format!(
                        "Dependency references unknown task '{task_name}'"
                    )));
                }
            } else {
                return Err(Error::validation(format!(
                    "Dependency references task '{task_name}' but no tasks are defined"
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
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    // Test types for validation
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
    enum TestTaskState {
        Idle,
        Running,
        Completed,
        Failed,
    }
    
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    struct TestAction;

    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    struct TestEvent;

    struct TestService {
        event_tx: async_channel::Sender<TestEvent>,
        event_rx: async_channel::Receiver<TestEvent>,
    }
    
    impl Default for TestService {
        fn default() -> Self {
            let (tx, rx) = async_channel::unbounded();
            Self {
                event_tx: tx,
                event_rx: rx,
            }
        }
    }

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
        
        fn event_stream(&self) -> async_channel::Receiver<Self::Event> {
            self.event_rx.clone()
        }

        async fn dispatch_action(
            &self,
            _action: Self::Action,
        ) -> Result<(), Error> {
            self.event_tx.send(TestEvent).await.unwrap();
            Ok(())
        }
    }

    // Test task for validation
    struct TestTask {
        state: std::sync::Arc<std::sync::Mutex<TestTaskState>>,
    }
    
    impl TestTask {
        fn new() -> Self {
            Self {
                state: std::sync::Arc::new(std::sync::Mutex::new(TestTaskState::Idle)),
            }
        }
    }

    #[async_trait]
    impl DeploymentTask for TestTask {
        type State = TestTaskState;
        const TASK_TYPE: &'static str = "test-task";
        
        async fn execute(&self) -> Result<async_channel::Receiver<Self::State>, Error> {
            let (tx, rx) = async_channel::bounded(3);
            let state = self.state.clone();
            
            // Simulate task execution
            smol::spawn(async move {
                let _ = tx.send(TestTaskState::Running).await;
                if let Ok(mut s) = state.lock() {
                    *s = TestTaskState::Running;
                }
                
                smol::Timer::after(std::time::Duration::from_millis(10)).await;
                
                let _ = tx.send(TestTaskState::Completed).await;
                if let Ok(mut s) = state.lock() {
                    *s = TestTaskState::Completed;
                }
            }).detach();
            
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
            assert!(
                err.to_string()
                    .contains("unknown service_type 'unknown-type'")
            );
        }
    }

    #[smol_potat::test]
    async fn test_validation_with_valid_service() {
        let mut builder = BaseDaemon::builder();
        builder
            .register_service("test-instance".to_string(), TestService::default())
            .unwrap();

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

        assert!(daemon.json_service_registry().get("test-instance").is_some());
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
        builder
            .register_task("test-task-instance".to_string(), TestTask::new())
            .unwrap();

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

        assert!(daemon.json_task_registry().list_types().contains(&"test-task"));
    }

    #[smol_potat::test]
    async fn test_validation_missing_service_dependency() {
        let mut builder = BaseDaemon::builder();
        builder
            .register_service("test-instance".to_string(), TestService::default())
            .unwrap();

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

        let result = builder.with_test_mode().with_config(config).build().await;

        assert!(result.is_err());
        if let Err(err) = result {
            assert!(
                err.to_string()
                    .contains("unknown service 'missing-service'")
            );
        }
    }

    #[smol_potat::test]
    async fn test_validation_missing_task_dependency() {
        let mut builder = BaseDaemon::builder();
        builder
            .register_service("test-instance".to_string(), TestService::default())
            .unwrap();

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

        let result = builder.with_test_mode().with_config(config).build().await;

        assert!(result.is_err());
        if let Err(err) = result {
            assert!(err.to_string().contains("no tasks are defined"));
        }
    }

    #[smol_potat::test]
    async fn test_validation_valid_mixed_dependencies() {
        let mut builder = BaseDaemon::builder();
        builder
            .register_service("svc1".to_string(), TestService::default())
            .unwrap();
        builder
            .register_service("svc2".to_string(), TestService::default())
            .unwrap();
        builder
            .register_task("task1".to_string(), TestTask::new())
            .unwrap();

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

        assert!(daemon.json_service_registry().get("svc1").is_some());
        assert!(daemon.json_service_registry().get("svc2").is_some());
    }

    #[smol_potat::test]
    async fn test_validation_task_with_service_dependency() {
        let mut builder = BaseDaemon::builder();
        builder
            .register_service("test-svc".to_string(), TestService::default())
            .unwrap();
        builder
            .register_task("task1".to_string(), TestTask::new())
            .unwrap();

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

        assert!(daemon.json_task_registry().list_types().contains(&"test-task"));
    }

    // NEW: Integration tests for mixed dependencies

    #[smol_potat::test]
    async fn test_circular_dependency_detection() {
        let mut builder = BaseDaemon::builder();
        builder
            .register_service("svc1".to_string(), TestService::default())
            .unwrap();
        builder
            .register_service("svc2".to_string(), TestService::default())
            .unwrap();

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

        assert!(daemon.json_service_registry().get("svc1").is_some());
    }

    #[smol_potat::test]
    async fn test_complex_dependency_chain() {
        let mut builder = BaseDaemon::builder();

        // Register multiple services
        builder
            .register_service("db".to_string(), TestService::default())
            .unwrap();
        builder
            .register_service("cache".to_string(), TestService::default())
            .unwrap();
        builder
            .register_service("api".to_string(), TestService::default())
            .unwrap();
        builder
            .register_service("web".to_string(), TestService::default())
            .unwrap();

        // Register tasks
        builder
            .register_task("db-migrate".to_string(), TestTask::new())
            .unwrap();
        builder
            .register_task("cache-warm".to_string(), TestTask::new())
            .unwrap();

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

        assert!(daemon.json_service_registry().get("db").is_some());
        assert!(daemon.json_service_registry().get("web").is_some());
        assert_eq!(daemon.json_task_registry().list_types().len(), 1);
    }

    #[smol_potat::test]
    async fn test_task_depending_on_task() {
        let mut builder = BaseDaemon::builder();
        builder
            .register_task("task1".to_string(), TestTask::new())
            .unwrap();
        builder
            .register_task("task2".to_string(), TestTask::new())
            .unwrap();

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

        assert_eq!(daemon.json_task_registry().list_types().len(), 1); // Only one type registered
    }
}
