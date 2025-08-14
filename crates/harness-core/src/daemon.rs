//! Core daemon abstractions and base implementation
//!
//! This module provides the `Daemon` trait and `BaseDaemon` implementation
//! that serves as the foundation for domain-specific daemons.

use async_runtime_compat::{AsyncSpawner, Task, prelude::*};
use async_trait::async_trait;
use serde_json::Value;
use service_orchestration::{
    DependencyGraph, DependencyNode, ServiceConfig, ServiceStatus, StackConfig,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

use crate::config_traits::{ServiceFromConfig, TaskFromConfig};
use crate::service::{JsonService, JsonServiceRegistry, Service};
use crate::task::YamlTask;
use crate::task::{DeploymentTask, JsonTaskRegistry};
use crate::typed_registry::{TypedServiceRegistry, TypedTaskRegistry};
use crate::websocket_dispatch::WebSocketServer;
use crate::{Error, ServiceManager};
use service_orchestration::TaskConfig;
use std::result::Result;

/// Core daemon trait that all harness daemons must implement
#[async_trait]
pub trait Daemon: Send + Sync {
    /// Start the daemon
    async fn start(&self) -> Result<(), Error>;

    /// Stop the daemon gracefully
    async fn stop(&self) -> Result<(), Error>;

    /// Get the WebSocket endpoint this daemon listens on
    fn endpoint(&self) -> SocketAddr;

    /// Get the service manager
    fn service_manager(&self) -> &ServiceManager;
}

/// Trait for daemons that can auto-wire their built-in types
pub trait AutoWire {
    /// Auto-wire all built-in service and task types for this daemon
    ///
    /// Implementations should attempt to wire all their known types.
    /// Types that don't exist in the configuration will be silently skipped.
    fn auto_wire_types(builder: &mut DaemonBuilder) -> Result<(), Error>;
}

/// Base daemon implementation that provides core functionality
pub struct BaseDaemon {
    /// Service manager for orchestrating services
    service_manager: ServiceManager,

    /// Registry of JSON-wrapped services (for WebSocket boundary)
    json_service_registry: Arc<JsonServiceRegistry>,

    /// Registry of JSON-wrapped tasks (for WebSocket boundary)
    json_task_registry: JsonTaskRegistry,

    /// Type-safe registry of concrete services
    typed_service_registry: Arc<TypedServiceRegistry>,

    /// Type-safe registry of concrete tasks
    typed_task_registry: Arc<TypedTaskRegistry>,

    /// WebSocket server address
    endpoint: SocketAddr,

    /// Whether the daemon is running
    running: Arc<std::sync::atomic::AtomicBool>,

    /// Stack configuration if provided
    stack_config: Option<StackConfig>,

    /// WebSocket server task handle and shutdown channel
    ws_server_handle: Arc<futures::lock::Mutex<Option<Task<()>>>>,
    ws_shutdown_tx: Arc<futures::lock::Mutex<Option<async_channel::Sender<()>>>>,
}

impl AutoWire for BaseDaemon {
    fn auto_wire_types(builder: &mut DaemonBuilder) -> Result<(), Error> {
        // Wire generic task types that are available in harness-core
        builder.wire_task_type::<YamlTask>()?;

        Ok(())
    }
}

impl BaseDaemon {
    /// Create a new daemon builder with a stack configuration
    pub fn builder(stack_config: StackConfig) -> DaemonBuilder {
        DaemonBuilder::new(stack_config)
    }

    /// Get the service manager
    pub fn service_manager(&self) -> &ServiceManager {
        &self.service_manager
    }

    /// Get the endpoint
    pub fn endpoint(&self) -> SocketAddr {
        self.endpoint
    }

    /// Get the JSON service registry
    pub fn json_service_registry(&self) -> &JsonServiceRegistry {
        &*self.json_service_registry
    }

    /// Get the JSON task registry
    pub fn json_task_registry(&self) -> &JsonTaskRegistry {
        &self.json_task_registry
    }

    /// Get a typed service instance by name
    ///
    /// This allows implementers to access concrete service types directly
    /// without going through JSON serialization.
    ///
    /// # Example
    /// ```ignore
    /// let graph_node = daemon.get_service::<GraphNodeService>("graph-node-1");
    /// ```
    pub fn get_service<S>(&self, name: &str) -> Option<Arc<S>>
    where
        S: Service + 'static,
    {
        self.typed_service_registry.get_service::<S>(name)
    }

    /// Get a typed task instance by name
    ///
    /// This allows implementers to access concrete task types directly
    /// without going through JSON serialization.
    pub fn get_task<T>(&self, name: &str) -> Option<Arc<T>>
    where
        T: DeploymentTask + 'static,
    {
        self.typed_task_registry.get_task::<T>(name)
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
                        depends_on: service_instance.orchestration.depends_on.clone(),
                        health_check: service_instance.orchestration.health_check.clone(),
                    };

                    // Start the service via ServiceManager
                    let (_event_receiver, _running_service) = self
                        .service_manager
                        .launch_service(&name, service_config, &AsyncSpawner::new())
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
        // Use the runtime-agnostic spawner

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
            // Check if task is already completed
            let is_complete = self.json_task_registry.validate(task_name).await?;

            if is_complete {
                info!("Task {} is already complete, skipping execution", task_name);
                return Ok(());
            }

            // Execute the task using the JsonTaskRegistry which handles spawning
            info!("Executing registered task: {}", task_name);

            // Execute using JsonTaskRegistry's execute method which handles spawning
            let spawner = AsyncSpawner::new();
            let state_rx = self.json_task_registry.execute(task_name, &spawner).await?;

            // Wait for task to complete by consuming all state updates
            let mut last_state = None;
            while let Ok(state) = state_rx.recv().await {
                info!("Task {} state: {:?}", task_name, state);
                last_state = Some(state);
            }

            // Check if the task failed by examining the last state
            if let Some(final_state) = last_state {
                // Check if the state indicates failure
                // We check the state as a JSON value since we don't know the concrete type
                if let serde_json::Value::String(state_str) = &final_state {
                    if state_str == "Failed" {
                        return Err(Error::action(format!("Task {} failed", task_name)));
                    }
                }
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
impl Daemon for BaseDaemon {
    async fn start(&self) -> Result<(), Error> {
        info!("Starting base daemon on {}", self.endpoint);

        // Mark as running
        self.running
            .store(true, std::sync::atomic::Ordering::SeqCst);

        // Start WebSocket server
        let ws_server = WebSocketServer::new(self.json_service_registry.clone(), self.endpoint);
        let shutdown_tx = ws_server.shutdown_handle();

        // Spawn the WebSocket server
        let spawner = AsyncSpawner::new();
        let handle = spawner.spawn_with_handle(Box::pin(async move {
            if let Err(e) = ws_server.run().await {
                tracing::error!("WebSocket server error: {}", e);
            }
        }));

        // Store the handle and shutdown channel
        *self.ws_server_handle.lock().await = Some(handle);
        *self.ws_shutdown_tx.lock().await = Some(shutdown_tx);

        info!("Base daemon started successfully");
        Ok(())
    }

    async fn stop(&self) -> Result<(), Error> {
        info!("Stopping base daemon");

        // Mark as stopped
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);

        // Shutdown WebSocket server
        if let Some(shutdown_tx) = self.ws_shutdown_tx.lock().await.take() {
            let _ = shutdown_tx.send(()).await;
        }

        // Wait for the server to stop
        if let Some(handle) = self.ws_server_handle.lock().await.take() {
            handle.await;
        }

        info!("Base daemon stopped");
        Ok(())
    }

    fn endpoint(&self) -> SocketAddr {
        self.endpoint
    }

    fn service_manager(&self) -> &ServiceManager {
        &self.service_manager
    }
}

/// Builder for creating daemon instances
pub struct DaemonBuilder {
    endpoint: SocketAddr,
    state_dir: Option<std::path::PathBuf>,
    json_service_registry: JsonServiceRegistry,
    json_task_registry: JsonTaskRegistry,
    typed_service_registry: TypedServiceRegistry,
    typed_task_registry: TypedTaskRegistry,
    stack_config: StackConfig,
    #[cfg(test)]
    test_mode: bool,
}

impl DaemonBuilder {
    /// Create a new daemon builder with a stack configuration
    pub fn new(stack_config: StackConfig) -> Self {
        Self {
            endpoint: "127.0.0.1:9443".parse().unwrap(),
            state_dir: None,
            json_service_registry: JsonServiceRegistry::new(),
            json_task_registry: JsonTaskRegistry::new(),
            typed_service_registry: TypedServiceRegistry::new(),
            typed_task_registry: TypedTaskRegistry::new(),
            stack_config,
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

    /// Register a service with the JSON service registry
    /// Note: Services should implement JsonService trait (typically via #[json_actions] macro)
    pub fn register_json_service(
        &mut self,
        instance_name: String,
        service: Box<dyn JsonService>,
    ) -> Result<&mut Self, Error> {
        // Log the service registration
        tracing::info!("Registering service '{}'", instance_name);

        // Register with the JSON service registry
        self.json_service_registry
            .register_json_service(instance_name, service)?;

        Ok(self)
    }

    /// Register a service from configuration using ServiceFromConfig trait
    pub fn register_service_from_config<S>(
        &mut self,
        instance_name: String,
        config: &ServiceConfig,
    ) -> Result<&mut Self, Error>
    where
        S: Service
            + ServiceFromConfig
            + crate::action::ServiceJsonActions
            + crate::service::HasDispatchJson
            + 'static,
    {
        // Create the service from config
        let service = S::from_config(config)?;

        // Register in typed registry first
        self.typed_service_registry
            .register(instance_name.clone(), service)?;

        // Get it back and register in JSON registry for WebSocket
        // We need to create a new instance for JSON registry since we moved it
        let service_for_json = S::from_config(config)?;
        self.json_service_registry
            .register(instance_name, service_for_json)?;
        Ok(self)
    }

    /// Wire up all services of a given type from the stored configuration
    pub fn wire_service_type<S>(&mut self) -> Result<&mut Self, Error>
    where
        S: Service
            + ServiceFromConfig
            + crate::action::ServiceJsonActions
            + crate::service::HasDispatchJson
            + 'static,
    {
        let config = &self.stack_config;

        // Find all services matching the specified type
        let matching_services: Vec<(String, service_orchestration::ServiceConfig)> = config
            .services
            .iter()
            .filter(|(_, service_instance)| service_instance.service_type == S::SERVICE_TYPE)
            .map(|(name, service_instance)| {
                let mut config = service_instance.orchestration.clone();
                // Set the service name from the map key if not already set
                if config.name.is_empty() {
                    config.name = name.clone();
                }
                (name.clone(), config)
            })
            .collect();

        if matching_services.is_empty() {
            tracing::debug!(
                "No services of type '{}' found in configuration",
                S::SERVICE_TYPE
            );
            return Ok(self);
        }

        // Register each matching service
        for (instance_name, service_config) in matching_services {
            // Check if already registered (for idempotency)
            if self.json_service_registry.get(&instance_name).is_some() {
                tracing::debug!("Service '{}' already registered, skipping", instance_name);
                continue;
            }

            tracing::info!(
                "Wiring service '{}' of type '{}'",
                instance_name,
                S::SERVICE_TYPE
            );

            // Create the service from config
            let service = S::from_config(&service_config)?;

            // Register in typed registry
            self.typed_service_registry
                .register(instance_name.clone(), service)?;

            // Create another instance for JSON registry
            let service_for_json = S::from_config(&service_config)?;
            self.json_service_registry
                .register(instance_name, service_for_json)?;
        }

        Ok(self)
    }

    /// Register a task with the task stack
    pub fn register_task<T>(&mut self, task_name: String, task: T) -> Result<&mut Self, Error>
    where
        T: DeploymentTask + Clone + 'static,
        T::State: schemars::JsonSchema,
    {
        // Log the task registration
        tracing::info!("Registering task '{}'", task_name);

        // Register in typed registry
        self.typed_task_registry
            .register(task_name.clone(), task.clone())?;

        // Register in JSON registry for WebSocket
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
        T: DeploymentTask + TaskFromConfig + Clone + 'static,
        T::State: schemars::JsonSchema,
    {
        // Create the task from config
        let task = T::from_config(config)?;

        // Use the existing register_task method
        self.register_task(task_name, task)
    }

    /// Wire up all tasks of a given type from the stored configuration
    pub fn wire_task_type<T>(&mut self) -> Result<&mut Self, Error>
    where
        T: DeploymentTask + TaskFromConfig + Clone + 'static,
        T::State: schemars::JsonSchema,
    {
        let config = &self.stack_config;

        // Find all tasks matching the specified type
        let matching_tasks: Vec<(String, TaskConfig)> = config
            .tasks
            .iter()
            .filter(|(_, task_config)| task_config.task_type == T::TASK_TYPE)
            .map(|(name, task_config)| (name.clone(), task_config.clone()))
            .collect();

        if matching_tasks.is_empty() {
            tracing::debug!("No tasks of type '{}' found in configuration", T::TASK_TYPE);
            return Ok(self);
        }

        // Register each matching task
        for (task_name, task_config) in matching_tasks {
            // Check if already registered (for idempotency)
            if self.json_task_registry.get(&task_name).is_some() {
                tracing::debug!("Task '{}' already registered, skipping", task_name);
                continue;
            }

            tracing::info!("Wiring task '{}' of type '{}'", task_name, T::TASK_TYPE);

            self.register_task_from_config::<T>(task_name, &task_config)?;
        }

        Ok(self)
    }

    /// Auto-wire types for a specific daemon implementation
    pub fn with_auto_wire<D: AutoWire>(&mut self) -> Result<&mut Self, Error> {
        D::auto_wire_types(self)?;
        Ok(self)
    }

    /// Build the daemon
    pub async fn build(mut self) -> Result<BaseDaemon, Error> {
        info!("Building daemon with endpoint {}", self.endpoint);

        // Auto-wire base daemon types
        BaseDaemon::auto_wire_types(&mut self)?;

        // Validate configuration
        self.validate_config()?;

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

        Ok(BaseDaemon {
            service_manager,
            json_service_registry: Arc::new(self.json_service_registry),
            json_task_registry: self.json_task_registry,
            typed_service_registry: Arc::new(self.typed_service_registry),
            typed_task_registry: Arc::new(self.typed_task_registry),
            endpoint: self.endpoint,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            stack_config: Some(self.stack_config),
            ws_server_handle: Arc::new(futures::lock::Mutex::new(None)),
            ws_shutdown_tx: Arc::new(futures::lock::Mutex::new(None)),
        })
    }

    /// Validate configuration - only check that dependency references are valid
    fn validate_config(&self) -> Result<(), Error> {
        let config = &self.stack_config;

        // Validate service dependencies
        info!(
            "Validating {} service configurations",
            config.services.len()
        );
        for (name, service_config) in &config.services {
            // Validate dependencies using the strongly-typed Dependency enum
            for dep in &service_config.orchestration.depends_on {
                let resolved = dep.resolve();
                match resolved {
                    service_orchestration::Dependency::Service { service } => {
                        if !config.services.contains_key(&service) {
                            return Err(Error::validation(format!(
                                "Service '{name}' depends on unknown service '{service}'"
                            )));
                        }
                    }
                    service_orchestration::Dependency::Task { task } => {
                        if !config.tasks.contains_key(&task) {
                            return Err(Error::validation(format!(
                                "Service '{name}' depends on unknown task '{task}'"
                            )));
                        }
                    }
                    service_orchestration::Dependency::Namespaced(_) => {
                        unreachable!("resolve() should never return Namespaced")
                    }
                }
            }
        }

        // Validate task dependencies
        info!("Validating {} task configurations", config.tasks.len());
        for (name, task_config) in &config.tasks {
            // Validate dependencies using the strongly-typed Dependency enum
            for dep in &task_config.depends_on {
                let resolved = dep.resolve();
                match resolved {
                    service_orchestration::Dependency::Service { service } => {
                        if !config.services.contains_key(&service) {
                            return Err(Error::validation(format!(
                                "Task '{name}' depends on unknown service '{service}'"
                            )));
                        }
                    }
                    service_orchestration::Dependency::Task { task } => {
                        if !config.tasks.contains_key(&task) {
                            return Err(Error::validation(format!(
                                "Task '{name}' depends on unknown task '{task}'"
                            )));
                        }
                    }
                    service_orchestration::Dependency::Namespaced(_) => {
                        unreachable!("resolve() should never return Namespaced")
                    }
                }
            }
        }

        Ok(())
    }
}

// Note: DaemonBuilder no longer implements Default since it requires a StackConfig
// Use DaemonBuilder::new(config) instead

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::{Service, ServiceEvents};
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

    impl Service for TestService {
        const SERVICE_TYPE: &'static str = "test-service";

        fn name(&self) -> &str {
            "test-service"
        }

        fn description(&self) -> &str {
            "Test service"
        }
    }

    #[async_trait]
    impl ServiceEvents for TestService {
        type Event = TestEvent;

        fn event_stream(&self) -> async_channel::Receiver<Self::Event> {
            self.event_rx.clone()
        }
    }

    impl TestService {
        async fn dispatch_test_action(&self, _action: TestAction) -> Result<(), Error> {
            self.event_tx.send(TestEvent).await.unwrap();
            Ok(())
        }
    }

    // Implement JsonService for testing
    #[async_trait]
    impl crate::service::JsonService for TestService {
        fn name(&self) -> &str {
            "test-service"
        }

        fn description(&self) -> &str {
            "Test service for unit tests"
        }

        fn available_actions(&self) -> Vec<crate::service::ActionDescriptor> {
            vec![crate::service::ActionDescriptor {
                name: "test_action".to_string(),
                description: "Test action".to_string(),
                input_schema: serde_json::json!({}),
                event_schema: serde_json::json!({}),
            }]
        }

        async fn dispatch_json(
            &self,
            action: &str,
            _input: serde_json::Value,
        ) -> Result<
            (
                async_channel::Receiver<serde_json::Value>,
                std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
            ),
            Error,
        > {
            if action != "test_action" {
                return Err(Error::service_type(format!("Unknown action: {}", action)));
            }

            let (tx, rx) = async_channel::bounded(1);
            self.dispatch_test_action(TestAction).await?;

            // Create a future that sends the result
            let future = async move {
                let _ = tx.send(serde_json::json!({})).await;
            };

            Ok((rx, Box::pin(future)))
        }

        fn has_setup(&self) -> bool {
            false
        }

        fn has_events(&self) -> bool {
            true
        }

        fn event_schema(&self) -> Option<serde_json::Value> {
            Some(serde_json::json!({
                "type": "object"
            }))
        }
    }

    // Test task for validation
    #[derive(Clone)]
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
            })
            .detach();

            Ok(rx)
        }
    }

    #[smol_potat::test]
    async fn test_daemon_builder() {
        let config = StackConfig {
            name: "test-stack".to_string(),
            description: None,
            services: std::collections::HashMap::new(),
            tasks: std::collections::HashMap::new(),
        };
        let daemon = BaseDaemon::builder(config)
            .with_test_mode()
            .with_endpoint("127.0.0.1:8080".parse().unwrap())
            .build()
            .await
            .unwrap();

        assert_eq!(daemon.endpoint().port(), 8080);
    }

    #[smol_potat::test]
    async fn test_validation_with_valid_service() {
        let config = StackConfig {
            name: "test-stack".to_string(),
            description: None,
            services: std::collections::HashMap::new(),
            tasks: std::collections::HashMap::new(),
        };
        let mut builder = BaseDaemon::builder(config);
        builder
            .register_json_service(
                "test-instance".to_string(),
                Box::new(TestService::default()),
            )
            .unwrap();

        let daemon = builder.with_test_mode().build().await.unwrap();

        assert!(daemon.json_service_registry.get("test-instance").is_some());
    }

    #[smol_potat::test]
    async fn test_validation_with_valid_task() {
        let config = StackConfig {
            name: "test-stack".to_string(),
            description: None,
            services: std::collections::HashMap::new(),
            tasks: std::collections::HashMap::new(),
        };
        let mut builder = BaseDaemon::builder(config);
        builder
            .register_task("test-task-instance".to_string(), TestTask::new())
            .unwrap();

        let config = json!({
            "tasks": {
                "test-task-instance": {
                    "task_type": "test-task",
                    "depends_on": []
                }
            }
        });

        let daemon = builder.with_test_mode().build().await.unwrap();

        assert!(
            daemon
                .json_task_registry()
                .list_types()
                .contains(&"test-task")
        );
    }

    #[smol_potat::test]
    async fn test_validation_valid_mixed_dependencies() {
        let config = StackConfig {
            name: "test-stack".to_string(),
            description: None,
            services: std::collections::HashMap::new(),
            tasks: std::collections::HashMap::new(),
        };
        let mut builder = BaseDaemon::builder(config);
        builder
            .register_json_service("svc1".to_string(), Box::new(TestService::default()))
            .unwrap()
            .register_json_service("svc2".to_string(), Box::new(TestService::default()))
            .unwrap();
        builder
            .register_task("task1".to_string(), TestTask::new())
            .unwrap();

        let config = json!({
            "services": {
                "svc1": {
                    "service_type": "test-service",
                    "depends_on": []
                },
                "svc2": {
                    "service_type": "test-service",
                    "depends_on": [
                        { "service": "svc1" },
                        { "task": "task1" }
                    ]
                }
            },
            "tasks": {
                "task1": {
                    "task_type": "test-task",
                    "depends_on": []
                }
            }
        });

        let daemon = builder.with_test_mode().build().await.unwrap();

        assert!(daemon.json_service_registry().get("svc1").is_some());
        assert!(daemon.json_service_registry().get("svc2").is_some());
    }

    #[smol_potat::test]
    async fn test_validation_task_with_service_dependency() {
        let config = StackConfig {
            name: "test-stack".to_string(),
            description: None,
            services: std::collections::HashMap::new(),
            tasks: std::collections::HashMap::new(),
        };
        let mut builder = BaseDaemon::builder(config);
        builder
            .register_json_service("test-svc".to_string(), Box::new(TestService::default()))
            .unwrap();
        builder
            .register_task("task1".to_string(), TestTask::new())
            .unwrap();

        let config = json!({
            "services": {
                "test-svc": {
                    "service_type": "test-service",
                    "depends_on": []
                }
            },
            "tasks": {
                "task1": {
                    "task_type": "test-task",
                    "depends_on": [
                        { "service": "test-svc" }
                    ]
                }
            }
        });

        let daemon = builder.with_test_mode().build().await.unwrap();

        assert!(
            daemon
                .json_task_registry()
                .list_types()
                .contains(&"test-task")
        );
    }

    // NEW: Integration tests for mixed dependencies

    #[smol_potat::test]
    async fn test_circular_dependency_detection() {
        let config = StackConfig {
            name: "test-stack".to_string(),
            description: None,
            services: std::collections::HashMap::new(),
            tasks: std::collections::HashMap::new(),
        };
        let mut builder = BaseDaemon::builder(config);
        builder
            .register_json_service("svc1".to_string(), Box::new(TestService::default()))
            .unwrap()
            .register_json_service("svc2".to_string(), Box::new(TestService::default()))
            .unwrap();

        // Service depends on itself indirectly through another service
        let config = json!({
            "services": {
                "svc1": {
                    "service_type": "test-service",
                    "depends_on": [
                        { "service": "svc2" }
                    ]
                },
                "svc2": {
                    "service_type": "test-service",
                    "depends_on": [
                        { "service": "svc1" }
                    ]
                }
            }
        });

        // This should pass validation as we don't detect circular dependencies here
        // That would be done at execution time by topological sort
        let daemon = builder.with_test_mode().build().await.unwrap();

        assert!(daemon.json_service_registry().get("svc1").is_some());
    }

    #[smol_potat::test]
    async fn test_complex_dependency_chain() {
        let config = StackConfig {
            name: "test-stack".to_string(),
            description: None,
            services: std::collections::HashMap::new(),
            tasks: std::collections::HashMap::new(),
        };
        let mut builder = BaseDaemon::builder(config);

        // Register multiple services
        builder
            .register_json_service("db".to_string(), Box::new(TestService::default()))
            .unwrap();
        builder
            .register_json_service("cache".to_string(), Box::new(TestService::default()))
            .unwrap();
        builder
            .register_json_service("api".to_string(), Box::new(TestService::default()))
            .unwrap();
        builder
            .register_json_service("web".to_string(), Box::new(TestService::default()))
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
                    "depends_on": []
                },
                "cache": {
                    "service_type": "test-service",
                    "depends_on": [
                        { "service": "db" },
                        { "task": "db-migrate" }
                    ]
                },
                "api": {
                    "service_type": "test-service",
                    "depends_on": [
                        { "service": "db" },
                        { "service": "cache" },
                        { "task": "cache-warm" }
                    ]
                },
                "web": {
                    "service_type": "test-service",
                    "depends_on": [
                        { "service": "api" }
                    ]
                }
            },
            "tasks": {
                "db-migrate": {
                    "task_type": "test-task",
                    "depends_on": [
                        { "service": "db" }
                    ]
                },
                "cache-warm": {
                    "task_type": "test-task",
                    "depends_on": [
                        { "service": "cache" }
                    ]
                }
            }
        });

        let daemon = builder.with_test_mode().build().await.unwrap();

        assert!(daemon.json_service_registry().get("db").is_some());
        assert!(daemon.json_service_registry().get("web").is_some());
        assert_eq!(daemon.json_task_registry().list_types().len(), 1);
    }

    #[smol_potat::test]
    async fn test_task_depending_on_task() {
        let config = StackConfig {
            name: "test-stack".to_string(),
            description: None,
            services: std::collections::HashMap::new(),
            tasks: std::collections::HashMap::new(),
        };
        let mut builder = BaseDaemon::builder(config);
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
                    "depends_on": []
                },
                "task2": {
                    "task_type": "test-task",
                    "depends_on": [
                        { "task": "task1" }
                    ]
                }
            }
        });

        let daemon = builder.with_test_mode().build().await.unwrap();

        assert_eq!(daemon.json_task_registry().list_types().len(), 1); // Only one type registered
    }
}
