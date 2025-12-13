//! Service manager for orchestrating heterogeneous services.
//!
//! The ServiceManager is the central orchestrator that manages the entire
//! service lifecycle across different execution environments.

use crate::{
    OrchestrationError,
    config::{HealthCheck, ServiceConfig, ServiceStatus, ServiceTarget},
    executors::{
        AttachedExecutor, AttachedService, DockerExecutor, ProcessExecutor, RunningService,
        ServiceExecutor, traits::EventStreamable,
    },
    health::{HealthChecker, HealthMonitor, HealthStatus},
    ports::{PortAllocator, PortRegistry},
    template::{RunContext, TemplateProcessor},
};
use async_channel::Receiver;
use async_runtime_compat::Spawner;
use command_executor::event::ProcessEvent;
use std::collections::HashMap;
use std::result::Result;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tracing::{debug, info, warn};

/// Central service orchestrator
///
/// Uses interior mutability for:
/// - `active_services`: RwLock allows concurrent reads, exclusive writes for service lifecycle
/// - `health_monitors`: RwLock for health check state updates
/// - `port_allocator`: RwLock for port allocation during service startup
/// - `run_context`: RwLock for run directory management
pub struct ServiceManager {
    /// Service executors by type
    executors: HashMap<String, Arc<dyn ServiceExecutor>>,
    /// Currently running services (interior mutability for &self methods)
    active_services: Arc<RwLock<HashMap<String, RunningService>>>,
    /// Service health monitors (interior mutability for health check updates)
    health_monitors: Arc<RwLock<HashMap<String, HealthMonitor>>>,
    /// Port allocator for dynamic port assignment (interior mutability for allocation)
    port_allocator: Arc<RwLock<PortAllocator>>,
    /// Run context for template processing and runtime file management
    run_context: Option<RunContext>,
}

impl ServiceManager {
    /// Create a new service manager
    pub async fn new() -> Result<Self, OrchestrationError> {
        debug!("Initializing ServiceManager");

        // Create harness directory if it doesn't exist
        let state_dir = dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("harness");

        Self::with_state_dir(state_dir).await
    }

    /// Create a new service manager with a specific state directory
    pub async fn with_state_dir(
        state_dir: impl Into<std::path::PathBuf>,
    ) -> Result<Self, OrchestrationError> {
        let state_dir = state_dir.into();
        debug!(
            "Initializing ServiceManager with state dir: {:?}",
            state_dir
        );

        std::fs::create_dir_all(&state_dir).map_err(OrchestrationError::Io)?;

        // Initialize executors
        let mut executors: HashMap<String, Arc<dyn ServiceExecutor>> = HashMap::new();
        executors.insert("process".to_string(), Arc::new(ProcessExecutor::new()));
        executors.insert("docker".to_string(), Arc::new(DockerExecutor::new()));

        Ok(Self {
            executors,
            active_services: Arc::new(RwLock::new(HashMap::new())),
            health_monitors: Arc::new(RwLock::new(HashMap::new())),
            port_allocator: Arc::new(RwLock::new(PortAllocator::with_default_range())),
            run_context: None,
        })
    }

    /// Create a new service manager for tests with a temporary directory
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn new_for_tests() -> Result<Self, OrchestrationError> {
        let temp_dir = tempfile::tempdir().map_err(OrchestrationError::Io)?;
        let state_dir = temp_dir.path().to_path_buf();

        // Keep the temp_dir alive by leaking it - it will be cleaned up when process exits
        std::mem::forget(temp_dir);

        Self::with_state_dir(state_dir).await
    }

    /// Allocate ports for all services that have port configurations.
    ///
    /// This should be called before launching any services to ensure all port
    /// references can be resolved. Services are processed in the order provided.
    ///
    /// # Example
    /// ```ignore
    /// // Collect all service configs
    /// let services: Vec<(&str, &ServiceConfig)> = config.services
    ///     .iter()
    ///     .map(|(name, svc)| (name.as_str(), &svc.orchestration))
    ///     .collect();
    ///
    /// // Allocate all ports upfront
    /// manager.allocate_ports_for_services(&services)?;
    /// ```
    pub fn allocate_ports_for_services(
        &self,
        services: &[(&str, &ServiceConfig)],
    ) -> Result<(), OrchestrationError> {
        let mut allocator = self.port_allocator.write().unwrap();

        for (name, config) in services {
            if let Some(port_config) = config.target.port_config() {
                info!("Allocating ports for service '{}': {:?}", name, port_config);
                allocator
                    .allocate_for_service(name, port_config)
                    .map_err(|e| OrchestrationError::Port(e))?;
            }
        }

        Ok(())
    }

    /// Get the port registry for inspection or testing.
    pub fn port_registry(&self) -> PortRegistry {
        self.port_allocator.read().unwrap().registry().clone()
    }

    /// Set the run context for template processing.
    ///
    /// This should be called before launching any services that have templates.
    /// The run context determines where generated config files are written.
    pub fn set_run_context(&mut self, run_context: RunContext) {
        info!("Setting run context: run_id={}", run_context.run_id);
        self.run_context = Some(run_context);
    }

    /// Get the run context if set.
    pub fn run_context(&self) -> Option<&RunContext> {
        self.run_context.as_ref()
    }

    /// Process all templates for a service.
    ///
    /// This reads template files from the templates directory, substitutes
    /// port references, and writes the output to the run directory.
    ///
    /// Returns the paths of generated config files.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No run context has been set
    /// - Template file cannot be read
    /// - Port substitution fails
    /// - Output file cannot be written
    pub fn process_templates(
        &self,
        service_name: &str,
        config: &ServiceConfig,
    ) -> Result<Vec<std::path::PathBuf>, OrchestrationError> {
        if config.templates.is_empty() {
            return Ok(vec![]);
        }

        let run_context = self.run_context.as_ref().ok_or_else(|| {
            OrchestrationError::Config(
                "Run context not set - call set_run_context before processing templates"
                    .to_string(),
            )
        })?;

        let allocator = self.port_allocator.read().unwrap();
        let processor = TemplateProcessor::new(&allocator, run_context);

        processor
            .process_templates(service_name, &config.templates)
            .map_err(|e| OrchestrationError::Config(format!("Template processing failed: {}", e)))
    }

    /// Substitute port and run context references in a service configuration.
    ///
    /// This replaces patterns like:
    /// - `{port.http}` and `{postgres.port.main}` for ports
    /// - `{run.config_dir}`, `{run.id}`, etc. for run context paths
    fn substitute_ports_in_config(
        &self,
        service_name: &str,
        config: ServiceConfig,
    ) -> Result<ServiceConfig, OrchestrationError> {
        let allocator = self.port_allocator.read().unwrap();

        // Helper to apply both port and run context substitution
        let substitute = |s: &str| -> Result<String, OrchestrationError> {
            let mut result = allocator
                .substitute_ports(s, Some(service_name))
                .map_err(OrchestrationError::Port)?;

            // Apply run context substitution if available
            if let Some(ctx) = &self.run_context {
                result = ctx.substitute(&result, service_name);
            }

            Ok(result)
        };

        let mut new_config = config.clone();

        match &mut new_config.target {
            ServiceTarget::Process { command, env, .. } => {
                // Substitute in environment variables
                for value in env.values_mut() {
                    *value = substitute(value)?;
                }

                // Substitute in command template if present
                match command {
                    crate::config::ProcessCommand::Template {
                        command_template, ..
                    } => {
                        *command_template = substitute(command_template)?;
                    }
                    crate::config::ProcessCommand::Legacy { command: cmd } => {
                        *cmd = substitute(cmd)?;
                    }
                    crate::config::ProcessCommand::Typed { .. } => {
                        // No command to substitute for typed tasks
                    }
                }
            }
            ServiceTarget::Docker {
                env,
                command_template,
                ..
            } => {
                // Substitute in environment variables
                for value in env.values_mut() {
                    *value = substitute(value)?;
                }
                // Substitute in command template if present
                if let Some(template) = command_template {
                    *template = substitute(template)?;
                }
            }
            // Other target types don't have port substitution
            _ => {}
        }

        // Substitute in health check if present
        if let Some(health_check) = &mut new_config.health_check {
            health_check.command = substitute(&health_check.command)?;
            for arg in &mut health_check.args {
                *arg = substitute(arg)?;
            }
        }

        // Populate allocated_ports from registry (for Docker executor)
        if let Some(ports) = allocator.registry().get(service_name) {
            new_config.allocated_ports = ports.clone();
        }

        Ok(new_config)
    }

    /// Launch a service with the given configuration
    pub async fn launch_service(
        &self,
        name: &str,
        config: ServiceConfig,
        spawner: &dyn Spawner,
    ) -> Result<(Receiver<ProcessEvent>, RunningService), OrchestrationError> {
        debug!("Launching service: {}", name);

        // Check if service is already running
        {
            let active = self.active_services.read().unwrap();
            if active.contains_key(name) {
                return Err(OrchestrationError::ServiceExists(name.to_string()));
            }
        }

        // Inject network configuration
        let network_config = self.inject_network_config(&config).await?;

        // Find appropriate executor
        let executor = self.find_executor(&network_config)?;

        // Start the service
        let running_service = executor.start(network_config.clone(), spawner).await?;

        // Get the event stream for this service
        let rx = executor.stream_events(&running_service, spawner).await?;

        // Start health monitoring if configured
        if let Some(health_check) = &network_config.health_check {
            let monitor = HealthMonitor::new(health_check.clone());
            self.health_monitors
                .write()
                .unwrap()
                .insert(name.to_string(), monitor);
        }

        // Store running service
        self.active_services
            .write()
            .unwrap()
            .insert(name.to_string(), running_service.clone());

        debug!("Successfully launched service: {}", name);
        Ok((rx, running_service))
    }

    /// Attach to an existing service
    pub async fn attach_service(
        &self,
        name: &str,
        config: ServiceConfig,
        spawner: &dyn Spawner,
    ) -> Result<(Receiver<ProcessEvent>, RunningService), OrchestrationError> {
        debug!("Attaching to service: {}", name);

        // Check if service is already managed
        {
            let active = self.active_services.read().unwrap();
            if active.contains_key(name) {
                return Err(OrchestrationError::ServiceExists(name.to_string()));
            }
        }

        // Inject network configuration
        let network_config = self.inject_network_config(&config).await?;

        // Use the AttachedExecutor for all attachment types
        let executor = AttachedExecutor::new();

        // Create a modified config with the appropriate metadata in env
        let mut attach_config = network_config.clone();
        match &mut attach_config.target {
            ServiceTarget::DockerAttach { container, env } => {
                env.insert("CONTAINER_NAME".to_string(), container.clone());
            }
            ServiceTarget::ProcessAttach {
                pid,
                process_name,
                env,
            } => {
                if let Some(p) = pid {
                    env.insert("PID".to_string(), p.to_string());
                }
                if let Some(pn) = process_name {
                    env.insert("PROCESS_NAME".to_string(), pn.clone());
                }
            }
            _ => {
                return Err(OrchestrationError::Config(format!(
                    "attach_service called with non-attach target: {:?}",
                    network_config.target
                )));
            }
        }

        let running_service = executor.attach(attach_config.clone(), spawner).await?;
        let event_stream = executor.stream_events(&running_service, spawner).await?;

        // event_stream is already a Receiver, just use it directly
        let rx = event_stream;

        // Start health monitoring if configured
        if let Some(health_check) = &network_config.health_check {
            let monitor = HealthMonitor::new(health_check.clone());
            self.health_monitors
                .write()
                .unwrap()
                .insert(name.to_string(), monitor);
        }

        // Store running service
        self.active_services
            .write()
            .unwrap()
            .insert(name.to_string(), running_service.clone());

        debug!("Successfully attached to service: {}", name);
        Ok((rx, running_service))
    }

    /// Stop a running service
    pub async fn stop_service(
        &self,
        name: &str,
        spawner: &dyn Spawner,
    ) -> Result<(), OrchestrationError> {
        debug!("Stopping service: {}", name);

        let service = {
            let mut active = self.active_services.write().unwrap();
            active.remove(name)
        };

        let Some(service) = service else {
            return Err(OrchestrationError::ServiceNotFound(name.to_string()));
        };

        // Find executor and stop service
        let executor = self.find_executor(&service.config)?;
        executor.stop(&service, spawner).await?;

        // Remove health monitor
        self.health_monitors.write().unwrap().remove(name);

        // Service has been removed from active_services, nothing more to do

        debug!("Successfully stopped service: {}", name);
        Ok(())
    }

    /// Stop all running services
    pub async fn stop_all_services(&self, spawner: &dyn Spawner) -> Result<(), OrchestrationError> {
        info!("Stopping all services...");

        // Get list of all running service names
        let service_names: Vec<String> = {
            let active = self.active_services.read().unwrap();
            active.keys().cloned().collect()
        };

        // Stop each service (errors are logged but we continue stopping others)
        for name in service_names {
            match self.stop_service(&name, spawner).await {
                Ok(_) => info!("Stopped service: {}", name),
                Err(e) => tracing::warn!("Failed to stop service {}: {}", name, e),
            }
        }

        info!("All services stopped");
        Ok(())
    }

    /// Wait for a service to become healthy
    pub async fn wait_for_health(
        &self,
        name: &str,
        timeout: Duration,
    ) -> Result<(), OrchestrationError> {
        let start = std::time::Instant::now();

        while start.elapsed() < timeout {
            match self.get_service_status(name).await? {
                ServiceStatus::Running => {
                    debug!("Service {} is healthy", name);
                    return Ok(());
                }
                ServiceStatus::Failed(reason) => {
                    return Err(OrchestrationError::Config(format!(
                        "Service {} failed: {}",
                        name, reason
                    )));
                }
                _ => {
                    // Still starting or in another transitional state
                    async_runtime_compat::prelude::sleep(Duration::from_secs(1)).await;
                }
            }
        }

        Err(OrchestrationError::Config(format!(
            "Service {} failed to become healthy within {:?}",
            name, timeout
        )))
    }

    /// Get the status of a service
    pub async fn get_service_status(
        &self,
        name: &str,
    ) -> Result<ServiceStatus, OrchestrationError> {
        // Check if service is in active services
        let active = self.active_services.read().unwrap();
        // TODO was this meant to be an is_some()?
        let Some(_service) = active.get(name) else {
            return Ok(ServiceStatus::Stopped);
        };

        // Check health if monitor exists
        if let Some(monitor) = self.health_monitors.read().unwrap().get(name) {
            match monitor.current_status() {
                HealthStatus::Healthy => Ok(ServiceStatus::Running),
                // TODO log unhealthy? why ignore msg here?
                HealthStatus::Unhealthy(_msg) => Ok(ServiceStatus::Unhealthy),
                HealthStatus::Unknown => Ok(ServiceStatus::Running), // Assume running if unknown
            }
        } else {
            // No health check, assume running if service exists
            Ok(ServiceStatus::Running)
        }
    }

    /// Get detailed information about a running service
    pub async fn get_service_info(
        &self,
        name: &str,
    ) -> Result<Option<RunningService>, OrchestrationError> {
        let active = self.active_services.read().unwrap();
        Ok(active.get(name).cloned())
    }

    /// Run health checks for all monitored services
    pub async fn run_health_checks(
        &self,
    ) -> Result<HashMap<String, HealthStatus>, OrchestrationError> {
        let mut results = HashMap::new();

        // Get all service names to check
        let services_to_check: Vec<String> = {
            let monitors = self.health_monitors.read().unwrap();
            monitors.keys().cloned().collect()
        };

        // Run health checks for each service
        for service_name in services_to_check {
            debug!("Running health check for service: {}", service_name);

            // Get the monitor config and check status
            let (has_monitor, config) = {
                let monitors = self.health_monitors.read().unwrap();
                if let Some(monitor) = monitors.get(&service_name) {
                    (true, monitor.config.clone())
                } else {
                    (false, HealthCheck::default())
                }
            };

            let status = if has_monitor {
                // Create a temporary checker to run the health check
                let checker = HealthChecker::new();
                match checker.check_health(&config).await {
                    Ok(HealthStatus::Healthy) => {
                        // Update consecutive failures to 0
                        if let Some(monitor) =
                            self.health_monitors.write().unwrap().get_mut(&service_name)
                        {
                            monitor.consecutive_failures = 0;
                            monitor.last_status = HealthStatus::Healthy;
                        }
                        HealthStatus::Healthy
                    }
                    Ok(status) => {
                        // Update monitor state
                        if let Some(monitor) =
                            self.health_monitors.write().unwrap().get_mut(&service_name)
                        {
                            if matches!(status, HealthStatus::Unhealthy(_)) {
                                monitor.consecutive_failures += 1;
                            }
                            monitor.last_status = status.clone();
                        }
                        status
                    }
                    Err(e) => {
                        warn!("Health check failed for service {}: {}", service_name, e);
                        let status = HealthStatus::Unhealthy(e.to_string());
                        if let Some(monitor) =
                            self.health_monitors.write().unwrap().get_mut(&service_name)
                        {
                            monitor.consecutive_failures += 1;
                            monitor.last_status = status.clone();
                        }
                        status
                    }
                }
            } else {
                HealthStatus::Unknown
            };

            results.insert(service_name, status);
        }

        Ok(results)
    }

    /// Inject network configuration into service config.
    ///
    /// Substitutes port references ({service.port.name}) with allocated ports.
    async fn inject_network_config(
        &self,
        config: &ServiceConfig,
    ) -> Result<ServiceConfig, OrchestrationError> {
        debug!("Injecting network config for service: {}", config.name);

        // Substitute port references in the config
        let config = self.substitute_ports_in_config(&config.name, config.clone())?;

        Ok(config)
    }

    /// Find the appropriate executor for a service configuration
    fn find_executor(
        &self,
        config: &ServiceConfig,
    ) -> Result<Arc<dyn ServiceExecutor>, OrchestrationError> {
        for executor in self.executors.values() {
            if executor.can_handle(config) {
                return Ok(executor.clone());
            }
        }

        Err(OrchestrationError::Config(format!(
            "No executor found for service target: {:?}",
            config.target
        )))
    }

    /// List all active services
    pub fn list_services(&self) -> Vec<String> {
        self.active_services
            .read()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServiceTarget;
    use std::collections::HashMap;

    #[smol_potat::test]
    async fn test_service_manager_creation() {
        let manager = ServiceManager::new_for_tests().await.unwrap();

        // Verify executors are registered
        assert!(manager.executors.contains_key("process"));
        assert!(manager.executors.contains_key("docker"));
    }

    #[smol_potat::test]
    async fn test_find_executor() {
        let manager = ServiceManager::new_for_tests().await.unwrap();

        let process_config = ServiceConfig {
            name: "test".to_string(),
            target: ServiceTarget::Process {
                command: crate::config::ProcessCommand::Legacy {
                    command: "echo".to_string(),
                },
                env: HashMap::new(),
                ports: HashMap::new(),
                resources: None,
                working_dir: None,
                complete_if: None,
            },
            depends_on: vec![],
            health_check: None,
            templates: vec![],
            allocated_ports: HashMap::new(),
        };

        let executor = manager.find_executor(&process_config).unwrap();
        assert!(executor.can_handle(&process_config));
    }

    #[smol_potat::test]
    async fn test_service_not_found() {
        let manager = ServiceManager::new_for_tests().await.unwrap();

        use async_runtime_compat::smol::SmolSpawner;
        let result = manager.stop_service("nonexistent", &SmolSpawner).await;
        assert!(matches!(
            result,
            Err(OrchestrationError::ServiceNotFound(_))
        ));
    }

    #[smol_potat::test]
    async fn test_list_services_empty() {
        let manager = ServiceManager::new_for_tests().await.unwrap();

        let services = manager.list_services();
        assert!(services.is_empty());
    }
}
