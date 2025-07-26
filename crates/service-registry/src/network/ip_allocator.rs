//! IP address allocation for WireGuard mesh network

use crate::error::{Error, Result};
use ipnet::IpNet;
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// IP address allocator for the WireGuard subnet
pub struct IpAllocator {
    /// The subnet to allocate from
    subnet: IpNet,

    /// Allocated IPs by service name
    allocations: HashMap<String, IpAddr>,

    /// Reverse mapping of IP to service
    ip_to_service: HashMap<IpAddr, String>,

    /// Set of all allocated IPs for fast lookup
    allocated_ips: HashSet<IpAddr>,
}

impl IpAllocator {
    /// Create a new IP allocator for the given subnet
    pub fn new(subnet: IpNet) -> Result<Self> {
        let mut allocator = Self {
            subnet,
            allocations: HashMap::new(),
            ip_to_service: HashMap::new(),
            allocated_ips: HashSet::new(),
        };

        // Reserve gateway IP (.1)
        allocator.reserve_gateway()?;

        Ok(allocator)
    }

    /// Reserve the gateway IP (.1)
    fn reserve_gateway(&mut self) -> Result<()> {
        let gateway_ip = match self.subnet {
            IpNet::V4(net) => {
                let base = u32::from(net.network());
                IpAddr::V4(Ipv4Addr::from(base + 1))
            }
            IpNet::V6(net) => {
                let base = u128::from(net.network());
                IpAddr::V6(Ipv6Addr::from(base + 1))
            }
        };

        self.allocated_ips.insert(gateway_ip);
        self.ip_to_service.insert(gateway_ip, "gateway".to_string());
        Ok(())
    }

    /// Allocate an IP address for a service
    pub fn allocate(&mut self, service_name: &str) -> Result<IpAddr> {
        // Check if already allocated
        if let Some(&ip) = self.allocations.get(service_name) {
            return Ok(ip);
        }

        // Find next available IP
        let ip = self.find_next_available()?;

        // Record allocation
        self.allocations.insert(service_name.to_string(), ip);
        self.ip_to_service.insert(ip, service_name.to_string());
        self.allocated_ips.insert(ip);

        Ok(ip)
    }

    /// Allocate a specific IP address for a service
    pub fn allocate_specific(&mut self, service_name: &str, ip: IpAddr) -> Result<()> {
        // Check if IP is in subnet
        if !self.subnet.contains(&ip) {
            return Err(Error::Package(format!(
                "IP {} is not in subnet {}",
                ip, self.subnet
            )));
        }

        // Check if already allocated
        if self.allocated_ips.contains(&ip) {
            if let Some(existing) = self.ip_to_service.get(&ip) {
                if existing != service_name {
                    return Err(Error::Package(format!(
                        "IP {} already allocated to {}",
                        ip, existing
                    )));
                }
            }
        }

        // Record allocation
        self.allocations.insert(service_name.to_string(), ip);
        self.ip_to_service.insert(ip, service_name.to_string());
        self.allocated_ips.insert(ip);

        Ok(())
    }

    /// Release an IP allocation
    pub fn release(&mut self, service_name: &str) -> Option<IpAddr> {
        if let Some(ip) = self.allocations.remove(service_name) {
            self.ip_to_service.remove(&ip);
            self.allocated_ips.remove(&ip);
            Some(ip)
        } else {
            None
        }
    }

    /// Get the IP allocated to a service
    pub fn get_allocation(&self, service_name: &str) -> Option<IpAddr> {
        self.allocations.get(service_name).copied()
    }

    /// Get the service using a specific IP
    pub fn get_service_by_ip(&self, ip: &IpAddr) -> Option<&str> {
        self.ip_to_service.get(ip).map(|s| s.as_str())
    }

    /// Find the next available IP address
    fn find_next_available(&mut self) -> Result<IpAddr> {
        // Always start from the beginning (.2) to reuse released IPs
        let start_ip = match self.subnet {
            IpNet::V4(net) => {
                let base = u32::from(net.network());
                IpAddr::V4(Ipv4Addr::from(base + 2))
            }
            IpNet::V6(net) => {
                let base = u128::from(net.network());
                IpAddr::V6(Ipv6Addr::from(base + 2))
            }
        };
        let mut current_ip = start_ip;

        loop {
            // Check if current IP is available
            if self.subnet.contains(&current_ip) && !self.allocated_ips.contains(&current_ip) {
                return Ok(current_ip);
            }

            // Move to next IP
            current_ip = self.increment_ip(current_ip)?;

            // Check if we've wrapped around
            if current_ip == start_ip {
                return Err(Error::Package("No available IPs in subnet".to_string()));
            }
        }
    }

    /// Increment an IP address
    fn increment_ip(&self, ip: IpAddr) -> Result<IpAddr> {
        match ip {
            IpAddr::V4(v4) => {
                let val = u32::from(v4);
                let next_val = val.wrapping_add(1);
                Ok(IpAddr::V4(Ipv4Addr::from(next_val)))
            }
            IpAddr::V6(v6) => {
                let val = u128::from(v6);
                let next_val = val.wrapping_add(1);
                Ok(IpAddr::V6(Ipv6Addr::from(next_val)))
            }
        }
    }

    /// Get all allocations
    pub fn all_allocations(&self) -> &HashMap<String, IpAddr> {
        &self.allocations
    }

    /// Get number of allocated IPs
    pub fn allocated_count(&self) -> usize {
        self.allocated_ips.len()
    }

    /// Get the subnet
    pub fn subnet(&self) -> &IpNet {
        &self.subnet
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ip_allocator_creation() {
        let subnet: IpNet = "10.42.0.0/16".parse().unwrap();
        let allocator = IpAllocator::new(subnet).unwrap();

        // Gateway should be reserved
        assert_eq!(allocator.allocated_count(), 1);
        assert_eq!(
            allocator.get_service_by_ip(&"10.42.0.1".parse().unwrap()),
            Some("gateway")
        );
    }

    #[test]
    fn test_ip_allocation() {
        let subnet: IpNet = "10.42.0.0/16".parse().unwrap();
        let mut allocator = IpAllocator::new(subnet).unwrap();

        // Allocate IPs for services
        let ip1 = allocator.allocate("service-1").unwrap();
        let ip2 = allocator.allocate("service-2").unwrap();
        let ip3 = allocator.allocate("service-3").unwrap();

        // Should get sequential IPs starting from .2
        assert_eq!(ip1, "10.42.0.2".parse::<IpAddr>().unwrap());
        assert_eq!(ip2, "10.42.0.3".parse::<IpAddr>().unwrap());
        assert_eq!(ip3, "10.42.0.4".parse::<IpAddr>().unwrap());

        // Allocating again should return same IP
        let ip1_again = allocator.allocate("service-1").unwrap();
        assert_eq!(ip1, ip1_again);
    }

    #[test]
    fn test_specific_allocation() {
        let subnet: IpNet = "10.42.0.0/16".parse().unwrap();
        let mut allocator = IpAllocator::new(subnet).unwrap();

        // Allocate specific IP
        let specific_ip = "10.42.0.100".parse().unwrap();
        allocator
            .allocate_specific("special-service", specific_ip)
            .unwrap();

        // Verify allocation
        assert_eq!(
            allocator.get_allocation("special-service"),
            Some(specific_ip)
        );
        assert_eq!(
            allocator.get_service_by_ip(&specific_ip),
            Some("special-service")
        );

        // Try to allocate same IP to different service
        let result = allocator.allocate_specific("other-service", specific_ip);
        assert!(result.is_err());
    }

    #[test]
    fn test_ip_release() {
        let subnet: IpNet = "10.42.0.0/16".parse().unwrap();
        let mut allocator = IpAllocator::new(subnet).unwrap();

        // Allocate and release
        let ip = allocator.allocate("temp-service").unwrap();
        assert_eq!(allocator.allocated_count(), 2); // gateway + service

        let released = allocator.release("temp-service");
        assert_eq!(released, Some(ip));
        assert_eq!(allocator.allocated_count(), 1); // just gateway

        // IP should be available again
        let new_ip = allocator.allocate("new-service").unwrap();
        assert_eq!(new_ip, ip);
    }

    #[test]
    fn test_subnet_bounds() {
        let subnet: IpNet = "10.42.0.0/24".parse().unwrap(); // Only 256 addresses
        let mut allocator = IpAllocator::new(subnet).unwrap();

        // Try to allocate outside subnet
        let result = allocator.allocate_specific("bad-service", "10.43.0.1".parse().unwrap());
        assert!(result.is_err());
    }
}
