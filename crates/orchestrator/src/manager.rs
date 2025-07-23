//! Service manager for orchestrating heterogeneous services.
//!
//! The ServiceManager is the central orchestrator that manages the entire
//! service lifecycle across different execution environments.

use crate::{
    config::{ServiceConfig, ServiceStatus},
    executors::{DockerExecutor, ProcessExecutor, RemoteExecutor, RunningService, ServiceExecutor},
    health::{HealthMonitor, HealthStatus},
    package::{DeployedPackage, PackageDeployer, RemoteTarget},
    Result,
};
use service_registry::{network::{NetworkManager, NetworkConfig}, registry::Registry};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::{Arc as StdArc, RwLock};
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
    active_services: StdArc<RwLock<HashMap<String, RunningService>>>,
    /// Service health monitors
    health_monitors: StdArc<RwLock<HashMap<String, HealthMonitor>>>,
    /// Package deployer for remote services
    package_deployer: PackageDeployer,
}

impl ServiceManager {
    /// Create a new service manager
    pub async fn new() -> Result<Self> {
        info!("Initializing ServiceManager");
        
        let registry = Registry::default();
        let network_config = NetworkConfig::default();
        let network_manager = NetworkManager::new(network_config)?;
        
        // Initialize executors
        let mut executors: HashMap<String, Arc<dyn ServiceExecutor>> = HashMap::new();
        executors.insert("process".to_string(), Arc::new(ProcessExecutor::new()));
        executors.insert("docker".to_string(), Arc::new(DockerExecutor::new()));
        executors.insert("remote".to_string(), Arc::new(RemoteExecutor::new()));
        
        Ok(Self {
            registry,
            network_manager,
            executors,
            active_services: StdArc::new(RwLock::new(HashMap::new())),
            health_monitors: StdArc::new(RwLock::new(HashMap::new())),
            package_deployer: PackageDeployer::new(),
        })
    }

    /// Start a service with the given configuration
    pub async fn start_service(&self, name: &str, config: ServiceConfig) -> Result<()> {
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
            self.health_monitors.write().unwrap().insert(name.to_string(), monitor);
        }
        
        // Store running service
        self.active_services.write().unwrap().insert(name.to_string(), running_service.clone());
        
        // Register with service registry
        // TODO: Convert RunningService to ServiceEntry format
        
        info!("Successfully started service: {}", name);
        Ok(())
    }

    /// Stop a running service
    pub async fn stop_service(&self, name: &str) -> Result<()> {
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
        
        // Unregister from service registry
        // TODO: Implement service registry removal
        
        info!("Successfully stopped service: {}", name);
        Ok(())
    }

    /// Deploy a package to a remote target
    pub async fn deploy_package(&self, target: RemoteTarget, package_path: &str) -> Result<DeployedPackage> {
        info!("Deploying package {} to {}", package_path, target.host);
        
        let deployed = self.package_deployer.deploy(package_path, target).await?;
        
        info!("Successfully deployed package: {}", deployed.manifest.name);
        Ok(deployed)
    }

    /// Get the status of a service
    pub async fn get_service_status(&self, name: &str) -> Result<ServiceStatus> {
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
    pub async fn list_services(&self) -> Result<Vec<String>> {
        let active = self.active_services.read().unwrap();
        Ok(active.keys().cloned().collect())
    }

    /// Get detailed information about a running service
    pub async fn get_service_info(&self, name: &str) -> Result<Option<RunningService>> {
        let active = self.active_services.read().unwrap();
        Ok(active.get(name).cloned())
    }

    /// Run health checks for all monitored services
    pub async fn run_health_checks(&self) -> Result<HashMap<String, HealthStatus>> {
        let mut results = HashMap::new();
        
        let mut monitors = self.health_monitors.write().unwrap();
        for (name, monitor) in monitors.iter_mut() {
            debug!("Running health check for service: {}", name);
            
            match monitor.check().await {
                Ok(status) => {
                    results.insert(name.clone(), status);
                }
                Err(e) => {
                    warn!("Health check failed for service {}: {}", name, e);
                    results.insert(name.clone(), HealthStatus::Unhealthy(e.to_string()));
                }
            }
        }
        
        Ok(results)
    }

    /// Inject network configuration into service config
    async fn inject_network_config(&self, config: &ServiceConfig) -> Result<ServiceConfig> {
        debug!("Injecting network config for service: {}", config.name);
        
        // TODO: Implement network injection:
        // 1. Register service with network manager
        // 2. Resolve dependency IPs
        // 3. Update environment variables
        
        // For now, return config as-is
        Ok(config.clone())
    }

    /// Find the appropriate executor for a service configuration
    fn find_executor(&self, config: &ServiceConfig) -> Result<Arc<dyn ServiceExecutor>> {
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

    /// Register a custom executor
    pub fn register_executor(&mut self, name: String, executor: Arc<dyn ServiceExecutor>) {
        info!("Registering custom executor: {}", name);
        self.executors.insert(name, executor);
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

    #[test]
    fn test_service_manager_creation() {
        smol::block_on(async {
            let manager = ServiceManager::new().await.unwrap();
            
            // Verify executors are registered
            assert!(manager.executors.contains_key("process"));
            assert!(manager.executors.contains_key("docker"));
            assert!(manager.executors.contains_key("remote"));
        });
    }

    #[test]
    fn test_find_executor() {
        smol::block_on(async {
            let manager = ServiceManager::new().await.unwrap();
            
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
        });
    }

    #[test]
    fn test_service_not_found() {
        smol::block_on(async {
            let manager = ServiceManager::new().await.unwrap();
            
            let result = manager.stop_service("nonexistent").await;
            assert!(matches!(result, Err(crate::Error::ServiceNotFound(_))));
        });
    }

    #[test]
    fn test_list_services_empty() {
        smol::block_on(async {
            let manager = ServiceManager::new().await.unwrap();
            
            let services = manager.list_services().await.unwrap();
            assert!(services.is_empty());
        });
    }
}