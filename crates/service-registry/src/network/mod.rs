//! Network topology detection and management
//!
//! This module handles:
//! - Service network discovery and classification
//! - IP address allocation and management
//! - WireGuard configuration generation
//! - Network path optimization

use crate::error::Result;
use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;

pub mod ip_allocator;
pub mod resolver;
pub mod topology;

// TODO: Implement wireguard module when adding WireGuard support
// #[cfg(feature = "wireguard")]
// pub mod wireguard;

pub use ip_allocator::IpAllocator;
pub use resolver::ServiceResolver;
pub use topology::{NetworkLocation, NetworkTopology};

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// WireGuard subnet for mesh network (e.g., 10.42.0.0/16)
    pub wireguard_subnet: IpNet,

    /// LAN interface for local network discovery (e.g., "eth0")
    pub lan_interface: String,

    /// DNS port (53 for root, 5353 for non-root)
    #[serde(default = "default_dns_port")]
    pub dns_port: u16,

    /// Enable WireGuard mesh network
    #[serde(default = "default_enable_wireguard")]
    pub enable_wireguard: bool,
}

fn default_dns_port() -> u16 {
    5353 // Non-root default
}

fn default_enable_wireguard() -> bool {
    true
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            wireguard_subnet: "10.42.0.0/16".parse().unwrap(),
            lan_interface: "eth0".to_string(),
            dns_port: default_dns_port(),
            enable_wireguard: true,
        }
    }
}

/// Network information for a service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceNetwork {
    /// Service name
    pub service_name: String,

    /// Network location type
    pub location: NetworkLocation,

    /// Host/Docker bridge IP
    pub host_ip: Option<IpAddr>,

    /// LAN IP if available
    pub lan_ip: Option<IpAddr>,

    /// WireGuard mesh IP
    pub wireguard_ip: Option<IpAddr>,

    /// WireGuard public key (if using WireGuard)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wireguard_public_key: Option<String>,

    /// Network interfaces available
    pub interfaces: Vec<String>,
}

/// Network manager coordinates all network operations
pub struct NetworkManager {
    config: NetworkConfig,
    topology: NetworkTopology,
    ip_allocator: IpAllocator,
    resolver: ServiceResolver,
}

impl NetworkManager {
    /// Create a new network manager
    pub fn new(config: NetworkConfig) -> Result<Self> {
        let ip_allocator = IpAllocator::new(config.wireguard_subnet.clone())?;
        let topology = NetworkTopology::new();
        let resolver = ServiceResolver::new();

        Ok(Self {
            config,
            topology,
            ip_allocator,
            resolver,
        })
    }

    /// Discover network topology from current services
    pub async fn discover_topology(&mut self) -> Result<&NetworkTopology> {
        self.topology.discover(&self.config).await?;
        Ok(&self.topology)
    }

    /// Register a service with its network information
    pub async fn register_service(&mut self, service: ServiceNetwork) -> Result<()> {
        // Allocate WireGuard IP if needed
        if matches!(service.location, NetworkLocation::WireGuard { .. }) {
            if service.wireguard_ip.is_none() {
                let ip = self.ip_allocator.allocate(&service.service_name)?;
                // Would update service here in real implementation
            }
        }

        self.topology.add_service(service.clone());
        self.resolver.add_service(service);
        Ok(())
    }

    /// Resolve the best IP address for service-to-service communication
    pub fn resolve_service_ip(&self, from_service: &str, to_service: &str) -> Result<IpAddr> {
        self.resolver
            .resolve(from_service, to_service, &self.topology)
    }

    /// Get all services requiring WireGuard
    pub fn services_requiring_wireguard(&self) -> Vec<&ServiceNetwork> {
        self.topology.services_requiring_wireguard()
    }

    /// Check if WireGuard is needed for any services
    pub fn requires_wireguard(&self) -> bool {
        self.topology.requires_wireguard()
    }

    /// Generate environment variables for a service
    pub fn generate_environment(&self, service_name: &str) -> Result<HashMap<String, String>> {
        let mut env = HashMap::new();

        // Add all service addresses
        for other_service in self.topology.all_services() {
            if other_service.service_name == service_name {
                continue;
            }

            let ip = self.resolve_service_ip(service_name, &other_service.service_name)?;
            let key = format!(
                "{}_ADDR",
                other_service.service_name.to_uppercase().replace('-', "_")
            );
            env.insert(key, ip.to_string());
        }

        Ok(env)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_config_default() {
        let config = NetworkConfig::default();
        assert_eq!(config.wireguard_subnet.to_string(), "10.42.0.0/16");
        assert_eq!(config.lan_interface, "eth0");
        assert_eq!(config.dns_port, 5353);
        assert!(config.enable_wireguard);
    }

    #[test]
    fn test_network_manager_creation() {
        let config = NetworkConfig::default();
        let manager = NetworkManager::new(config).unwrap();
        assert!(!manager.requires_wireguard());
    }
}
