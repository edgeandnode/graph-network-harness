//! Service manager for orchestrating heterogeneous services.
//!
//! The ServiceManager is the central orchestrator that manages the entire
//! service lifecycle across different execution environments.

use crate::{
    Error,
    config::{HealthCheck, ServiceConfig, ServiceStatus},
    executors::{DockerExecutor, ProcessExecutor, RunningService, ServiceExecutor},
    health::{HealthChecker, HealthMonitor, HealthStatus},
    package::{DeployedPackage, PackageDeployer, RemoteTarget},
};
use service_registry::{
    network::{NetworkConfig, NetworkManager},
    registry::Registry,
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

/// Central service orchestrator
pub struct ServiceManager {
    /// Service registry for service discovery
    registry: Registry,
    /// Network manager for topology management
    network_manager: NetworkManager,
    /// Service executors by type
    executors: HashMap<String, Arc<dyn ServiceExecutor>>,
    /// Currently running services
    active_services: Arc<RwLock<HashMap<String, RunningService>>>,
    /// Service health monitors
    health_monitors: Arc<RwLock<HashMap<String, HealthMonitor>>>,
    /// Package deployer for remote services
    package_deployer: PackageDeployer,
}

impl ServiceManager {
    /// Create a new service manager
    pub async fn new() -> std::result::Result<Self, Error> {
        info!("Initializing ServiceManager");

        // Create harness directory if it doesn't exist
        let state_dir = dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("harness");

        Self::with_state_dir(state_dir).await
    }

    /// Create a new service manager with a specific state directory
    pub async fn with_state_dir(
        state_dir: impl Into<std::path::PathBuf>,
    ) -> std::result::Result<Self, Error> {
        let state_dir = state_dir.into();
        info!(
            "Initializing ServiceManager with state dir: {:?}",
            state_dir
        );

        std::fs::create_dir_all(&state_dir).map_err(crate::Error::Io)?;

        // Create in-memory registry
        let registry = Registry::new().await;

        let network_config = NetworkConfig::default();
        let network_manager = NetworkManager::new(network_config)?;

        // Initialize executors
        let mut executors: HashMap<String, Arc<dyn ServiceExecutor>> = HashMap::new();
        executors.insert("process".to_string(), Arc::new(ProcessExecutor::new()));
        executors.insert("docker".to_string(), Arc::new(DockerExecutor::new()));

        Ok(Self {
            registry,
            network_manager,
            executors,
            active_services: Arc::new(RwLock::new(HashMap::new())),
            health_monitors: Arc::new(RwLock::new(HashMap::new())),
            package_deployer: PackageDeployer::new(),
        })
    }

    /// Create a new service manager for tests with a temporary directory
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn new_for_tests() -> std::result::Result<Self, Error> {
        let temp_dir = tempfile::tempdir().map_err(crate::Error::Io)?;
        let state_dir = temp_dir.path().to_path_buf();

        // Keep the temp_dir alive by leaking it - it will be cleaned up when process exits
        std::mem::forget(temp_dir);

        Self::with_state_dir(state_dir).await
    }

    /// Start a service with the given configuration
    pub async fn start_service(
        &self,
        name: &str,
        config: ServiceConfig,
    ) -> std::result::Result<RunningService, Error> {
        info!("Starting service: {}", name);

        // Check if service is already running
        {
            let active = self.active_services.read().unwrap();
            if active.contains_key(name) {
                return Err(crate::Error::ServiceExists(name.to_string()));
            }
        }

        // Inject network configuration
        let network_config = self.inject_network_config(&config).await?;

        // Find appropriate executor
        let executor = self.find_executor(&network_config)?;

        // Start the service
        let running_service = executor.start(network_config.clone()).await?;

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

        // Register with service registry
        let execution_info = match &config.target {
            crate::config::ServiceTarget::Process { binary, args, .. } => {
                service_registry::models::ExecutionInfo::ManagedProcess {
                    pid: running_service.pid,
                    command: binary.clone(),
                    args: args.clone(),
                }
            }
            crate::config::ServiceTarget::Docker { image, .. } => {
                service_registry::models::ExecutionInfo::DockerContainer {
                    container_id: running_service.container_id.clone(),
                    image: image.clone(),
                    name: Some(format!("orchestrator-{}", name)),
                }
            }
            _ => {
                // For remote services, we'll use ManagedProcess for now
                service_registry::models::ExecutionInfo::ManagedProcess {
                    pid: None,
                    command: name.to_string(),
                    args: vec![],
                }
            }
        };

        let service_entry = service_registry::models::ServiceEntry::new(
            name.to_string(),
            "1.0.0".to_string(), // Version could come from config
            execution_info,
            service_registry::models::Location::Local,
        )?;

        // Register the service
        if let Err(e) = self.registry.register(service_entry).await {
            warn!("Failed to register service with registry: {}", e);
        }

        info!("Successfully started service: {}", name);
        Ok(running_service)
    }

    /// Stop a running service
    pub async fn stop_service(&self, name: &str) -> std::result::Result<(), Error> {
        info!("Stopping service: {}", name);

        let service = {
            let mut active = self.active_services.write().unwrap();
            active.remove(name)
        };

        let Some(service) = service else {
            return Err(crate::Error::ServiceNotFound(name.to_string()));
        };

        // Find executor and stop service
        let executor = self.find_executor(&service.config)?;
        executor.stop(&service).await?;

        // Remove health monitor
        self.health_monitors.write().unwrap().remove(name);

        // Update service state in registry to stopped
        if let Err(e) = self
            .registry
            .update_state(name, service_registry::models::ServiceState::Stopped)
            .await
        {
            warn!("Failed to update service state in registry: {}", e);
        }

        info!("Successfully stopped service: {}", name);
        Ok(())
    }

    /// Deploy a package to a remote target
    pub async fn deploy_package(
        &self,
        target: RemoteTarget,
        package_path: &str,
    ) -> std::result::Result<DeployedPackage, Error> {
        info!("Deploying package {} to {}", package_path, target.host);

        let deployed = self.package_deployer.deploy(package_path, target).await?;

        info!("Successfully deployed package: {}", deployed.manifest.name);
        Ok(deployed)
    }

    /// Get the status of a service
    pub async fn get_service_status(
        &self,
        name: &str,
    ) -> std::result::Result<ServiceStatus, Error> {
        // First check if service exists in registry
        if let Ok(service_info) = self.registry.get(name).await {
            // Check if we have it in active services
            let active = self.active_services.read().unwrap();
            if !active.contains_key(name) {
                // Service is in registry but not in our active list
                // This means it was started in a previous run
                match service_info.state {
                    service_registry::models::ServiceState::Running => {
                        // Service claims to be running, verify it
                        // For now, we'll trust the registry
                        return Ok(ServiceStatus::Running);
                    }
                    service_registry::models::ServiceState::Stopped => {
                        return Ok(ServiceStatus::Stopped);
                    }
                    service_registry::models::ServiceState::Failed => {
                        return Ok(ServiceStatus::Failed("Service failed".to_string()));
                    }
                    _ => {
                        return Ok(ServiceStatus::Stopped);
                    }
                }
            }
        }

        let active = self.active_services.read().unwrap();
        let Some(service) = active.get(name) else {
            return Ok(ServiceStatus::Stopped);
        };

        // Check health if monitor exists
        if let Some(monitor) = self.health_monitors.read().unwrap().get(name) {
            match monitor.current_status() {
                HealthStatus::Healthy => Ok(ServiceStatus::Running),
                HealthStatus::Unhealthy(msg) => Ok(ServiceStatus::Unhealthy),
                HealthStatus::Unknown => Ok(ServiceStatus::Running), // Assume running if unknown
            }
        } else {
            // No health check, assume running if service exists
            Ok(ServiceStatus::Running)
        }
    }

    /// List all active services
    pub async fn list_services(&self) -> std::result::Result<Vec<String>, Error> {
        let active = self.active_services.read().unwrap();
        Ok(active.keys().cloned().collect())
    }

    /// Get detailed information about a running service
    pub async fn get_service_info(
        &self,
        name: &str,
    ) -> std::result::Result<Option<RunningService>, Error> {
        let active = self.active_services.read().unwrap();
        Ok(active.get(name).cloned())
    }

    /// Run health checks for all monitored services
    pub async fn run_health_checks(
        &self,
    ) -> std::result::Result<HashMap<String, HealthStatus>, Error> {
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

    /// Inject network configuration into service config
    async fn inject_network_config(
        &self,
        config: &ServiceConfig,
    ) -> std::result::Result<ServiceConfig, Error> {
        debug!("Injecting network config for service: {}", config.name);

        // TODO: Implement network injection:
        // 1. Register service with network manager
        // 2. Resolve dependency IPs
        // 3. Update environment variables

        // For now, return config as-is
        Ok(config.clone())
    }

    /// Find the appropriate executor for a service configuration
    fn find_executor(
        &self,
        config: &ServiceConfig,
    ) -> std::result::Result<Arc<dyn ServiceExecutor>, Error> {
        for executor in self.executors.values() {
            if executor.can_handle(config) {
                return Ok(executor.clone());
            }
        }

        Err(crate::Error::Config(format!(
            "No executor found for service target: {:?}",
            config.target
        )))
    }

    /// Get network manager reference
    pub fn network_manager(&self) -> &NetworkManager {
        &self.network_manager
    }

    /// Get service registry reference
    pub fn service_registry(&self) -> &Registry {
        &self.registry
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
                binary: "echo".to_string(),
                args: vec![],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![],
            health_check: None,
        };

        let executor = manager.find_executor(&process_config).unwrap();
        assert!(executor.can_handle(&process_config));
    }

    #[smol_potat::test]
    async fn test_service_not_found() {
        let manager = ServiceManager::new_for_tests().await.unwrap();

        let result = manager.stop_service("nonexistent").await;
        assert!(matches!(result, Err(crate::Error::ServiceNotFound(_))));
    }

    #[smol_potat::test]
    async fn test_list_services_empty() {
        let manager = ServiceManager::new_for_tests().await.unwrap();

        let services = manager.list_services().await.unwrap();
        assert!(services.is_empty());
    }
}
