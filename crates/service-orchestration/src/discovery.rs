//! Service discovery and configuration injection
//!
//! This module provides service discovery capabilities that integrate with
//! the orchestrator to enable services to find each other and inject
//! configuration based on discovered endpoints.

use crate::{Error, config::ServiceConfig};
use service_registry::{Endpoint, Registry, ServiceEntry, ServiceState as RegistryServiceState};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Service discovery provider that integrates with the registry
pub struct ServiceDiscovery {
    registry: Arc<Registry>,
}

impl ServiceDiscovery {
    /// Create a new service discovery provider
    pub fn new(registry: Arc<Registry>) -> Self {
        Self { registry }
    }

    /// Discover service endpoints by service type
    pub async fn discover_by_type(
        &self,
        service_type: &str,
    ) -> Result<Vec<ServiceEndpoint>, Error> {
        let services = self.registry.list().await;

        let endpoints: Vec<ServiceEndpoint> = services
            .into_iter()
            .filter(|s| {
                // Match by service name pattern or exact type
                s.name.contains(service_type) ||
                s.name == service_type ||
                // Also check if it's a prefixed service like "postgres-1"
                s.name.starts_with(&format!("{}-", service_type))
            })
            .filter(|s| matches!(s.state, RegistryServiceState::Running))
            .flat_map(|s| {
                s.endpoints.into_iter().map(move |ep| ServiceEndpoint {
                    service_name: s.name.clone(),
                    service_version: s.version.clone(),
                    endpoint: ep,
                })
            })
            .collect();

        info!(
            "Discovered {} endpoints for service type '{}'",
            endpoints.len(),
            service_type
        );
        Ok(endpoints)
    }

    /// Discover a specific service by name
    pub async fn discover_service(&self, name: &str) -> Result<Option<ServiceEntry>, Error> {
        let services = self.registry.list().await;

        let service = services.into_iter().find(|s| s.name == name);

        if let Some(ref s) = service {
            debug!("Found service '{}' in state {:?}", name, s.state);
        } else {
            debug!("Service '{}' not found", name);
        }

        Ok(service)
    }

    /// Wait for a service to become available
    pub async fn wait_for_service(
        &self,
        name: &str,
        max_attempts: u32,
    ) -> Result<ServiceEntry, Error> {
        use std::time::Duration;

        info!("Waiting for service '{}' to become available", name);

        for attempt in 1..=max_attempts {
            if let Some(service) = self.discover_service(name).await? {
                if matches!(service.state, RegistryServiceState::Running) {
                    info!("Service '{}' is now available", name);
                    return Ok(service);
                }
                debug!(
                    "Service '{}' found but not running (state: {:?})",
                    name, service.state
                );
            }

            if attempt < max_attempts {
                debug!(
                    "Attempt {}/{} - service '{}' not ready, waiting...",
                    attempt, max_attempts, name
                );
                #[cfg(feature = "smol")]
                smol::Timer::after(Duration::from_secs(1)).await;
                #[cfg(feature = "tokio")]
                tokio::time::sleep(Duration::from_secs(1)).await;
                #[cfg(feature = "async-std")]
                async_std::task::sleep(Duration::from_secs(1)).await;
            }
        }

        Err(Error::Other(format!(
            "Service '{}' did not become available after {} attempts",
            name, max_attempts
        )))
    }

    /// Build configuration for a service based on discovered dependencies
    pub async fn build_service_config(
        &self,
        service_config: &ServiceConfig,
    ) -> Result<HashMap<String, String>, Error> {
        let mut config = HashMap::new();

        // For each dependency, discover endpoints and add to config
        for dep in &service_config.dependencies {
            match dep {
                crate::config::Dependency::Service { service } => {
                    if let Some(dep_service) = self.discover_service(service).await? {
                        // Add all endpoints from the dependency to config
                        for (idx, endpoint) in dep_service.endpoints.iter().enumerate() {
                            let key = if idx == 0 {
                                // Primary endpoint uses simple key
                                format!("{}_endpoint", service.to_uppercase())
                            } else {
                                // Additional endpoints are numbered
                                format!("{}_{}_endpoint", service.to_uppercase(), endpoint.name)
                            };

                            let value = match endpoint.protocol {
                                service_registry::Protocol::Http => {
                                    format!("http://{}", endpoint.address)
                                }
                                service_registry::Protocol::Https => {
                                    format!("https://{}", endpoint.address)
                                }
                                service_registry::Protocol::Grpc => {
                                    format!("grpc://{}", endpoint.address)
                                }
                                service_registry::Protocol::Tcp => endpoint.address.to_string(),
                                service_registry::Protocol::WebSocket => {
                                    format!("ws://{}", endpoint.address)
                                }
                                service_registry::Protocol::Custom(ref proto) => {
                                    format!("{}://{}", proto, endpoint.address)
                                }
                            };

                            config.insert(key, value);
                        }

                        // Also add individual host and port for compatibility
                        if let Some(primary) = dep_service.endpoints.first() {
                            config.insert(
                                format!("{}_HOST", service.to_uppercase()),
                                primary.address.ip().to_string(),
                            );
                            config.insert(
                                format!("{}_PORT", service.to_uppercase()),
                                primary.address.port().to_string(),
                            );
                        }
                    } else {
                        warn!("Dependency service '{}' not found", service);
                    }
                }
                crate::config::Dependency::Task { .. } => {
                    // Tasks don't have endpoints, skip
                }
            }
        }

        info!(
            "Built configuration with {} entries for service",
            config.len()
        );
        Ok(config)
    }
}

/// Represents a discovered service endpoint
#[derive(Debug, Clone)]
pub struct ServiceEndpoint {
    /// Name of the service providing this endpoint
    pub service_name: String,
    /// Version of the service
    pub service_version: String,
    /// The endpoint details
    pub endpoint: Endpoint,
}

/// Configuration provider that can inject discovered values
pub struct ConfigurationProvider {
    discovery: ServiceDiscovery,
    static_config: HashMap<String, String>,
}

impl ConfigurationProvider {
    /// Create a new configuration provider
    pub fn new(registry: Arc<Registry>) -> Self {
        Self {
            discovery: ServiceDiscovery::new(registry),
            static_config: HashMap::new(),
        }
    }

    /// Add static configuration values
    pub fn add_static_config(&mut self, key: String, value: String) {
        self.static_config.insert(key, value);
    }

    /// Get configuration for a service, merging static and discovered values
    pub async fn get_service_config(
        &self,
        service_config: &ServiceConfig,
    ) -> Result<HashMap<String, String>, Error> {
        // Start with static config
        let mut config = self.static_config.clone();

        // Add discovered configuration
        let discovered = self.discovery.build_service_config(service_config).await?;
        config.extend(discovered);

        // Add service-specific overrides from the service config
        if let crate::config::ServiceTarget::Process { env, .. } = &service_config.target {
            config.extend(env.clone());
        }

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use service_registry::{ExecutionInfo, Location, Registry, ServiceEntry};

    async fn create_test_registry() -> Registry {
        let registry = Registry::new().await;

        // Register a postgres service
        let postgres_service = ServiceEntry {
            name: "postgres-1".to_string(),
            version: "14.0".to_string(),
            execution: ExecutionInfo::ManagedProcess {
                pid: Some(1234),
                command: "postgres".to_string(),
                args: vec![],
            },
            location: Location::Local,
            endpoints: vec![Endpoint {
                name: "primary".to_string(),
                address: "127.0.0.1:5432".parse().unwrap(),
                protocol: service_registry::Protocol::Tcp,
                metadata: HashMap::new(),
            }],
            depends_on: vec![],
            state: RegistryServiceState::Running,
            last_health_check: None,
            registered_at: Utc::now(),
            last_state_change: Utc::now(),
        };

        registry.register(postgres_service).await.unwrap();
        registry
    }

    #[cfg(feature = "smol")]
    #[smol_potat::test]
    async fn test_discover_by_type() {
        let registry = create_test_registry().await;
        let discovery = ServiceDiscovery::new(Arc::new(registry));

        let endpoints = discovery.discover_by_type("postgres").await.unwrap();
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].service_name, "postgres-1");
    }

    #[cfg(feature = "smol")]
    #[smol_potat::test]
    async fn test_build_service_config() {
        let registry = create_test_registry().await;
        let discovery = ServiceDiscovery::new(Arc::new(registry));

        let service_config = ServiceConfig {
            name: "app".to_string(),
            target: crate::config::ServiceTarget::Process {
                binary: "app".to_string(),
                args: vec![],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![crate::config::Dependency::Service {
                service: "postgres-1".to_string(),
            }],
            health_check: None,
        };

        let config = discovery
            .build_service_config(&service_config)
            .await
            .unwrap();

        assert_eq!(config.get("POSTGRES-1_HOST").unwrap(), "127.0.0.1");
        assert_eq!(config.get("POSTGRES-1_PORT").unwrap(), "5432");
        assert_eq!(config.get("POSTGRES-1_endpoint").unwrap(), "127.0.0.1:5432");
    }
}
