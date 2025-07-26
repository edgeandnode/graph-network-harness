//! Network topology detection and management

use super::{NetworkConfig, ServiceNetwork};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;

/// Network location type for a service
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NetworkLocation {
    /// Service running on the same host as the harness
    Local,

    /// Service on local area network, directly reachable
    RemoteLAN {
        /// LAN IP address
        ip: IpAddr,
    },

    /// Service requiring WireGuard tunnel
    WireGuard {
        /// Remote hostname or IP
        endpoint: String,
    },
}

/// Network topology tracks all services and their network locations
#[derive(Debug, Clone)]
pub struct NetworkTopology {
    /// All registered services
    services: HashMap<String, ServiceNetwork>,

    /// Network interfaces discovered on the host
    host_interfaces: Vec<NetworkInterface>,
}

/// Network interface information
#[derive(Debug, Clone)]
pub struct NetworkInterface {
    /// Interface name (e.g., "eth0", "wlan0")
    pub name: String,

    /// IP addresses assigned to this interface
    pub addresses: Vec<IpAddr>,

    /// Whether this is a virtual interface (Docker, etc.)
    pub is_virtual: bool,
}

impl NetworkTopology {
    /// Create a new empty topology
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
            host_interfaces: Vec::new(),
        }
    }

    /// Discover network topology from system
    pub async fn discover(&mut self, config: &NetworkConfig) -> Result<()> {
        // Discover network interfaces
        self.host_interfaces = self.discover_interfaces().await?;

        // Detect Docker networks
        self.detect_docker_networks().await?;

        // Scan for LAN services (if configured)
        if !config.lan_interface.is_empty() {
            self.scan_lan_services(config).await?;
        }

        Ok(())
    }

    /// Discover network interfaces on the host
    async fn discover_interfaces(&self) -> Result<Vec<NetworkInterface>> {
        // In a real implementation, this would use async-net or similar
        // For now, return a mock implementation
        Ok(vec![
            NetworkInterface {
                name: "lo".to_string(),
                addresses: vec!["127.0.0.1".parse().unwrap()],
                is_virtual: false,
            },
            NetworkInterface {
                name: "eth0".to_string(),
                addresses: vec!["192.168.1.100".parse().unwrap()],
                is_virtual: false,
            },
            NetworkInterface {
                name: "docker0".to_string(),
                addresses: vec!["172.17.0.1".parse().unwrap()],
                is_virtual: true,
            },
        ])
    }

    /// Detect Docker networks and containers
    async fn detect_docker_networks(&self) -> Result<()> {
        // Would integrate with Docker API to detect containers
        // For now, this is a placeholder
        Ok(())
    }

    /// Scan LAN for services
    async fn scan_lan_services(&self, _config: &NetworkConfig) -> Result<()> {
        // Would perform network discovery (ARP scan, mDNS, etc.)
        // For now, this is a placeholder
        Ok(())
    }

    /// Add a service to the topology
    pub fn add_service(&mut self, service: ServiceNetwork) {
        self.services.insert(service.service_name.clone(), service);
    }

    /// Get a service by name
    pub fn get_service(&self, name: &str) -> Option<&ServiceNetwork> {
        self.services.get(name)
    }

    /// Get all services
    pub fn all_services(&self) -> Vec<&ServiceNetwork> {
        self.services.values().collect()
    }

    /// Check if any services require WireGuard
    pub fn requires_wireguard(&self) -> bool {
        self.services
            .values()
            .any(|s| matches!(s.location, NetworkLocation::WireGuard { .. }))
    }

    /// Get all services requiring WireGuard
    pub fn services_requiring_wireguard(&self) -> Vec<&ServiceNetwork> {
        self.services
            .values()
            .filter(|s| matches!(s.location, NetworkLocation::WireGuard { .. }))
            .collect()
    }

    /// Determine if two services can communicate directly
    pub fn can_communicate_directly(&self, service_a: &str, service_b: &str) -> bool {
        let Some(a) = self.get_service(service_a) else {
            return false;
        };
        let Some(b) = self.get_service(service_b) else {
            return false;
        };

        match (&a.location, &b.location) {
            // Local services can always talk
            (NetworkLocation::Local, NetworkLocation::Local) => true,

            // LAN services can talk if on same network
            (NetworkLocation::RemoteLAN { .. }, NetworkLocation::RemoteLAN { .. }) => {
                // Would check if on same subnet
                true
            }

            // WireGuard required for any other combination
            _ => false,
        }
    }

    /// Get the LAN interface
    pub fn lan_interface(&self) -> Option<&NetworkInterface> {
        self.host_interfaces
            .iter()
            .find(|iface| !iface.is_virtual && iface.name != "lo")
    }
}

impl Default for NetworkTopology {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topology_creation() {
        let topology = NetworkTopology::new();
        assert_eq!(topology.all_services().len(), 0);
        assert!(!topology.requires_wireguard());
    }

    #[test]
    fn test_service_registration() {
        let mut topology = NetworkTopology::new();

        let service = ServiceNetwork {
            service_name: "test-service".to_string(),
            location: NetworkLocation::Local,
            host_ip: Some("172.17.0.2".parse().unwrap()),
            lan_ip: None,
            wireguard_ip: None,
            wireguard_public_key: None,
            interfaces: vec!["docker0".to_string()],
        };

        topology.add_service(service);
        assert_eq!(topology.all_services().len(), 1);
        assert!(!topology.requires_wireguard());
    }

    #[test]
    fn test_wireguard_detection() {
        let mut topology = NetworkTopology::new();

        let service = ServiceNetwork {
            service_name: "remote-service".to_string(),
            location: NetworkLocation::WireGuard {
                endpoint: "remote.example.com".to_string(),
            },
            host_ip: None,
            lan_ip: None,
            wireguard_ip: Some("10.42.0.2".parse().unwrap()),
            wireguard_public_key: Some("publickey123".to_string()),
            interfaces: vec![],
        };

        topology.add_service(service);
        assert!(topology.requires_wireguard());
        assert_eq!(topology.services_requiring_wireguard().len(), 1);
    }

    #[test]
    fn test_direct_communication() {
        let mut topology = NetworkTopology::new();

        // Add two local services
        topology.add_service(ServiceNetwork {
            service_name: "service-a".to_string(),
            location: NetworkLocation::Local,
            host_ip: Some("172.17.0.2".parse().unwrap()),
            lan_ip: None,
            wireguard_ip: None,
            wireguard_public_key: None,
            interfaces: vec![],
        });

        topology.add_service(ServiceNetwork {
            service_name: "service-b".to_string(),
            location: NetworkLocation::Local,
            host_ip: Some("172.17.0.3".parse().unwrap()),
            lan_ip: None,
            wireguard_ip: None,
            wireguard_public_key: None,
            interfaces: vec![],
        });

        assert!(topology.can_communicate_directly("service-a", "service-b"));
    }
}
