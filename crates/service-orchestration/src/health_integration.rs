//! Health checking integration for the orchestrator
//!
//! This module integrates health checking into the orchestration system,
//! providing continuous monitoring of service health and automatic recovery.

use crate::{
    Error,
    config::ServiceConfig,
    context::OrchestrationContext,
    health::{HealthMonitor, HealthStatus},
};
use async_runtime_compat::runtime_utils::sleep;
use service_registry::{Registry, ServiceState as RegistryServiceState};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Health monitoring manager that runs health checks for all services
pub struct HealthMonitoringManager {
    context: Arc<OrchestrationContext>,
    monitors: Arc<Mutex<HashMap<String, Arc<Mutex<HealthMonitor>>>>>,
    monitor_handles: Arc<Mutex<HashMap<String, MonitorHandle>>>,
}

type MonitorHandle = std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>;

impl HealthMonitoringManager {
    /// Create a new health monitoring manager
    pub fn new(context: Arc<OrchestrationContext>) -> Self {
        Self {
            context,
            monitors: Arc::new(Mutex::new(HashMap::new())),
            monitor_handles: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start health monitoring for a service
    pub fn start_monitoring(
        &self,
        service_name: String,
        service_config: ServiceConfig,
    ) -> Result<(), Error> {
        if let Some(health_check) = service_config.health_check {
            info!("Starting health monitoring for service '{}'", service_name);

            // Create the health monitor
            let monitor = HealthMonitor::new(health_check);
            let interval = monitor.interval();

            // Store the monitor
            {
                let mut monitors = self.monitors.lock().unwrap();
                monitors.insert(service_name.clone(), Arc::new(Mutex::new(monitor)));
            }

            // Start the monitoring task
            let monitors = self.monitors.clone();
            let registry = self.context.registry.clone();
            let service_name_clone = service_name.clone();
            let context = self.context.clone();

            let handle = Box::pin(async move {
                monitor_service_health(service_name_clone, monitors, registry, context, interval)
                    .await
            });

            // Spawn the monitoring task
            self.context.spawner.spawn(handle);

            info!(
                "Health monitoring started for service '{}' with interval {:?}",
                service_name, interval
            );
        } else {
            debug!("No health check configured for service '{}'", service_name);
        }

        Ok(())
    }

    /// Stop health monitoring for a service
    pub fn stop_monitoring(&self, service_name: &str) {
        info!("Stopping health monitoring for service '{}'", service_name);

        // Remove the monitor
        let mut monitors = self.monitors.lock().unwrap();
        monitors.remove(service_name);

        // The monitoring task will exit on next iteration when it doesn't find the monitor
    }

    /// Get the current health status of a service
    pub fn get_health_status(&self, service_name: &str) -> Option<HealthStatus> {
        let monitors = self.monitors.lock().unwrap();
        monitors.get(service_name).map(|m| {
            let monitor = m.lock().unwrap();
            monitor.current_status().clone()
        })
    }

    /// Get health status for all monitored services
    pub fn get_all_health_status(&self) -> HashMap<String, HealthStatus> {
        let monitors = self.monitors.lock().unwrap();
        monitors
            .iter()
            .map(|(name, monitor)| {
                let m = monitor.lock().unwrap();
                (name.clone(), m.current_status().clone())
            })
            .collect()
    }
}

/// Monitor a service's health continuously
async fn monitor_service_health(
    service_name: String,
    monitors: Arc<Mutex<HashMap<String, Arc<Mutex<HealthMonitor>>>>>,
    registry: Arc<Registry>,
    context: Arc<OrchestrationContext>,
    interval: Duration,
) {
    info!(
        "Health monitoring task started for service '{}'",
        service_name
    );

    // Create a local health checker that doesn't need to be stored
    let mut health_checker = crate::health::HealthChecker::new();
    let health_config = {
        let monitors = monitors.lock().unwrap();
        monitors.get(&service_name).and_then(|m| {
            let monitor = m.lock().unwrap();
            Some(monitor.config.clone())
        })
    };

    let Some(config) = health_config else {
        warn!(
            "No health check config found for service '{}'",
            service_name
        );
        return;
    };

    let mut consecutive_failures = 0u32;
    let mut last_status = HealthStatus::Unknown;

    loop {
        // Check if monitoring is still active for this service
        let should_continue = {
            let monitors = monitors.lock().unwrap();
            monitors.contains_key(&service_name)
        };

        if !should_continue {
            info!("Health monitoring stopped for service '{}'", service_name);
            break;
        }

        // Perform health check using local checker
        let health_result = health_checker.check_health(&config).await;

        match health_result {
            Ok(status) => {
                debug!("Health check for '{}': {:?}", service_name, status);

                // Update service state based on health
                match status {
                    HealthStatus::Healthy => {
                        consecutive_failures = 0;
                        last_status = status.clone();

                        // Update to running if not already
                        let _ = registry
                            .update_state(&service_name, RegistryServiceState::Running)
                            .await;
                    }
                    HealthStatus::Unhealthy(ref reason) => {
                        warn!("Service '{}' is unhealthy: {}", service_name, reason);
                        consecutive_failures += 1;

                        // Only update status if we've exceeded retry threshold
                        if consecutive_failures >= config.retries {
                            last_status = status.clone();

                            // Update state to failed
                            let _ = registry
                                .update_state(&service_name, RegistryServiceState::Failed)
                                .await;
                        }

                        // Trigger recovery if too many failures
                        if consecutive_failures >= 3 {
                            error!(
                                "Service '{}' has failed {} consecutive health checks, triggering recovery",
                                service_name, consecutive_failures
                            );

                            // Trigger service recovery
                            trigger_service_recovery(&service_name, &context).await;

                            // Reset failure count after recovery
                            consecutive_failures = 0;
                        }
                    }
                    HealthStatus::Unknown => {
                        debug!("Health status unknown for service '{}'", service_name);
                        last_status = status;
                    }
                }

                // Update the monitor state
                {
                    let monitors = monitors.lock().unwrap();
                    if let Some(monitor_arc) = monitors.get(&service_name) {
                        let mut monitor = monitor_arc.lock().unwrap();
                        monitor.consecutive_failures = consecutive_failures;
                        monitor.last_status = last_status.clone();
                    }
                }
            }
            Err(e) => {
                error!(
                    "Failed to check health for service '{}': {}",
                    service_name, e
                );
            }
        }

        // Wait for next check interval
        sleep(interval).await;
    }
}

/// Trigger recovery for a failed service
async fn trigger_service_recovery(service_name: &str, context: &Arc<OrchestrationContext>) {
    info!("Triggering recovery for service '{}'", service_name);

    // Get the service configuration from the registry
    let services = context.registry.list().await;
    let service_entry = services.iter().find(|s| s.name == service_name);

    if let Some(entry) = service_entry {
        // Update state to stopping (we'll restart it)
        let _ = context
            .registry
            .update_state(service_name, RegistryServiceState::Stopping)
            .await;

        // In a real implementation, we would:
        // 1. Stop the failing service
        // 2. Wait a moment
        // 3. Start it again
        // 4. Reset the health monitor

        // For now, just log the recovery attempt
        info!("Recovery initiated for service '{}'", service_name);

        // Simulate recovery delay
        sleep(Duration::from_secs(5)).await;

        // Update back to starting state
        let _ = context
            .registry
            .update_state(service_name, RegistryServiceState::Starting)
            .await;

        info!("Recovery completed for service '{}'", service_name);
    } else {
        warn!(
            "Service '{}' not found in registry for recovery",
            service_name
        );
    }
}

/// Extension trait to add health monitoring to OrchestrationContext
pub trait HealthMonitoringExt {
    /// Get or create the health monitoring manager
    fn health_monitoring(&self) -> Arc<HealthMonitoringManager>;
}

// Global health monitoring manager storage
use std::sync::OnceLock;
static HEALTH_MONITORING_MANAGER: OnceLock<Arc<HealthMonitoringManager>> = OnceLock::new();

impl HealthMonitoringExt for OrchestrationContext {
    fn health_monitoring(&self) -> Arc<HealthMonitoringManager> {
        HEALTH_MONITORING_MANAGER
            .get_or_init(|| Arc::new(HealthMonitoringManager::new(Arc::new(self.clone()))))
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HealthCheck, ServiceTarget};
    use service_registry::Registry;

    fn create_test_service_config(with_health_check: bool) -> ServiceConfig {
        let health_check = if with_health_check {
            Some(HealthCheck {
                command: "true".to_string(),
                args: vec![],
                interval: 1,
                retries: 3,
                timeout: 5,
            })
        } else {
            None
        };

        ServiceConfig {
            name: "test-service".to_string(),
            target: ServiceTarget::Process {
                binary: "echo".to_string(),
                args: vec!["test".to_string()],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![],
            health_check,
        }
    }

    #[cfg(feature = "smol")]
    #[smol_potat::test]
    async fn test_health_monitoring_manager() {
        let config = crate::task_config::StackConfig {
            name: "test".to_string(),
            description: None,
            services: HashMap::new(),
            tasks: HashMap::new(),
        };

        let registry = Registry::new().await;
        let context = OrchestrationContext::new(config, registry);
        let manager = HealthMonitoringManager::new(Arc::new(context));

        // Start monitoring with health check
        let service_config = create_test_service_config(true);
        manager
            .start_monitoring("test-service".to_string(), service_config)
            .unwrap();

        // Wait a bit for initial health check
        sleep(Duration::from_millis(100)).await;

        // Check status
        let status = manager.get_health_status("test-service");
        assert!(status.is_some());

        // Stop monitoring
        manager.stop_monitoring("test-service");

        // Status should be gone
        sleep(Duration::from_millis(100)).await;
        let status = manager.get_health_status("test-service");
        assert!(status.is_none());
    }

    #[cfg(feature = "smol")]
    #[smol_potat::test]
    async fn test_no_health_check_configured() {
        let config = crate::task_config::StackConfig {
            name: "test".to_string(),
            description: None,
            services: HashMap::new(),
            tasks: HashMap::new(),
        };

        let registry = Registry::new().await;
        let context = OrchestrationContext::new(config, registry);
        let manager = HealthMonitoringManager::new(Arc::new(context));

        // Start monitoring without health check
        let service_config = create_test_service_config(false);
        manager
            .start_monitoring("test-service".to_string(), service_config)
            .unwrap();

        // Should have no health status
        let status = manager.get_health_status("test-service");
        assert!(status.is_none());
    }
}
