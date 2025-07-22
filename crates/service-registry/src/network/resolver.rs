//! Service IP resolution logic

use super::{NetworkLocation, NetworkTopology, ServiceNetwork};
use crate::error::{Error, Result};
use std::collections::HashMap;
use std::net::IpAddr;

/// Service resolver determines the best IP address for service-to-service communication
pub struct ServiceResolver {
    /// Cache of resolution decisions
    resolution_cache: HashMap<(String, String), IpAddr>,
}

impl ServiceResolver {
    /// Create a new service resolver
    pub fn new() -> Self {
        Self {
            resolution_cache: HashMap::new(),
        }
    }
    
    /// Add a service to the resolver
    pub fn add_service(&mut self, _service: ServiceNetwork) {
        // Clear cache when services change
        self.resolution_cache.clear();
    }
    
    /// Resolve the best IP address for communication from one service to another
    pub fn resolve(
        &self,
        from_service: &str,
        to_service: &str,
        topology: &NetworkTopology,
    ) -> Result<IpAddr> {
        // Check cache first
        let cache_key = (from_service.to_string(), to_service.to_string());
        if let Some(&ip) = self.resolution_cache.get(&cache_key) {
            return Ok(ip);
        }
        
        // Get service information
        let from = topology.get_service(from_service)
            .ok_or_else(|| Error::ServiceNotFound(from_service.to_string()))?;
        let to = topology.get_service(to_service)
            .ok_or_else(|| Error::ServiceNotFound(to_service.to_string()))?;
        
        // Determine best IP based on network locations
        let ip = self.determine_best_ip(from, to)?;
        
        // Cache the result (in a real implementation)
        // self.resolution_cache.insert(cache_key, ip);
        
        Ok(ip)
    }
    
    /// Determine the best IP address based on network topology
    fn determine_best_ip(&self, from: &ServiceNetwork, to: &ServiceNetwork) -> Result<IpAddr> {
        match (&from.location, &to.location) {
            // Both services are local - use host/Docker IP
            (NetworkLocation::Local, NetworkLocation::Local) => {
                to.host_ip.ok_or_else(|| {
                    Error::Package(format!("Service {} has no host IP", to.service_name))
                })
            }
            
            // Both services on LAN - use LAN IP
            (NetworkLocation::RemoteLAN { .. }, NetworkLocation::RemoteLAN { .. }) => {
                to.lan_ip.ok_or_else(|| {
                    Error::Package(format!("Service {} has no LAN IP", to.service_name))
                })
            }
            
            // From local to LAN - use LAN IP if available
            (NetworkLocation::Local, NetworkLocation::RemoteLAN { .. }) => {
                to.lan_ip.ok_or_else(|| {
                    Error::Package(format!("Service {} has no LAN IP", to.service_name))
                })
            }
            
            // From LAN to local - check if harness has LAN interface
            (NetworkLocation::RemoteLAN { .. }, NetworkLocation::Local) => {
                // If the local service has a LAN IP, use it
                if let Some(lan_ip) = to.lan_ip {
                    Ok(lan_ip)
                } else {
                    // Otherwise use host IP and hope for routing
                    to.host_ip.ok_or_else(|| {
                        Error::Package(format!("Service {} has no accessible IP", to.service_name))
                    })
                }
            }
            
            // Any WireGuard endpoint - must use WireGuard IP
            (_, NetworkLocation::WireGuard { .. }) | 
            (NetworkLocation::WireGuard { .. }, _) => {
                to.wireguard_ip.ok_or_else(|| {
                    Error::Package(format!("Service {} has no WireGuard IP", to.service_name))
                })
            }
        }
    }
    
    /// Resolve multiple services at once
    pub fn resolve_many(
        &self,
        from_service: &str,
        to_services: &[String],
        topology: &NetworkTopology,
    ) -> Result<HashMap<String, IpAddr>> {
        let mut results = HashMap::new();
        
        for to_service in to_services {
            let ip = self.resolve(from_service, to_service, topology)?;
            results.insert(to_service.clone(), ip);
        }
        
        Ok(results)
    }
    
    /// Clear the resolution cache
    pub fn clear_cache(&mut self) {
        self.resolution_cache.clear();
    }
}

impl Default for ServiceResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::NetworkTopology;
    
    fn create_test_topology() -> NetworkTopology {
        let mut topology = NetworkTopology::new();
        
        // Add local service
        topology.add_service(ServiceNetwork {
            service_name: "local-service".to_string(),
            location: NetworkLocation::Local,
            host_ip: Some("172.17.0.2".parse().unwrap()),
            lan_ip: None,
            wireguard_ip: None,
            wireguard_public_key: None,
            interfaces: vec!["docker0".to_string()],
        });
        
        // Add LAN service
        topology.add_service(ServiceNetwork {
            service_name: "lan-service".to_string(),
            location: NetworkLocation::RemoteLAN { 
                ip: "192.168.1.50".parse().unwrap() 
            },
            host_ip: None,
            lan_ip: Some("192.168.1.50".parse().unwrap()),
            wireguard_ip: None,
            wireguard_public_key: None,
            interfaces: vec!["eth0".to_string()],
        });
        
        // Add WireGuard service
        topology.add_service(ServiceNetwork {
            service_name: "remote-service".to_string(),
            location: NetworkLocation::WireGuard { 
                endpoint: "remote.example.com".to_string() 
            },
            host_ip: None,
            lan_ip: None,
            wireguard_ip: Some("10.42.0.10".parse().unwrap()),
            wireguard_public_key: Some("pubkey123".to_string()),
            interfaces: vec!["wg0".to_string()],
        });
        
        topology
    }
    
    #[test]
    fn test_local_to_local_resolution() {
        let topology = create_test_topology();
        let resolver = ServiceResolver::new();
        
        // Add another local service
        let mut topology = topology;
        topology.add_service(ServiceNetwork {
            service_name: "local-service-2".to_string(),
            location: NetworkLocation::Local,
            host_ip: Some("172.17.0.3".parse().unwrap()),
            lan_ip: None,
            wireguard_ip: None,
            wireguard_public_key: None,
            interfaces: vec!["docker0".to_string()],
        });
        
        let ip = resolver.resolve("local-service", "local-service-2", &topology).unwrap();
        assert_eq!(ip, "172.17.0.3".parse::<IpAddr>().unwrap());
    }
    
    #[test]
    fn test_local_to_lan_resolution() {
        let topology = create_test_topology();
        let resolver = ServiceResolver::new();
        
        let ip = resolver.resolve("local-service", "lan-service", &topology).unwrap();
        assert_eq!(ip, "192.168.1.50".parse::<IpAddr>().unwrap());
    }
    
    #[test]
    fn test_wireguard_resolution() {
        let topology = create_test_topology();
        let resolver = ServiceResolver::new();
        
        // Any service to WireGuard service should use WireGuard IP
        let ip = resolver.resolve("local-service", "remote-service", &topology).unwrap();
        assert_eq!(ip, "10.42.0.10".parse::<IpAddr>().unwrap());
        
        let ip = resolver.resolve("lan-service", "remote-service", &topology).unwrap();
        assert_eq!(ip, "10.42.0.10".parse::<IpAddr>().unwrap());
    }
    
    #[test]
    fn test_resolve_many() {
        let topology = create_test_topology();
        let resolver = ServiceResolver::new();
        
        let to_services = vec![
            "lan-service".to_string(),
            "remote-service".to_string(),
        ];
        
        let results = resolver.resolve_many("local-service", &to_services, &topology).unwrap();
        
        assert_eq!(results.len(), 2);
        assert_eq!(results.get("lan-service"), Some(&"192.168.1.50".parse::<IpAddr>().unwrap()));
        assert_eq!(results.get("remote-service"), Some(&"10.42.0.10".parse::<IpAddr>().unwrap()));
    }
    
    #[test]
    fn test_missing_service() {
        let topology = create_test_topology();
        let resolver = ServiceResolver::new();
        
        let result = resolver.resolve("local-service", "non-existent", &topology);
        assert!(result.is_err());
        
        let result = resolver.resolve("non-existent", "local-service", &topology);
        assert!(result.is_err());
    }
}