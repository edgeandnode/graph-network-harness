//! Port allocation and management for services.
//!
//! This module provides dynamic port allocation to avoid collisions when running
//! multiple test environments. Services can request `auto` ports which are
//! allocated from a configurable range, or specify fixed ports.
//!
//! # Port References
//!
//! Services can reference ports using the syntax:
//! - `{port.name}` - reference own port by name
//! - `{service.port.name}` - reference another service's port
//!
//! # Example YAML
//!
//! ```yaml
//! services:
//!   postgres:
//!     ports:
//!       main: auto
//!   graph-node:
//!     ports:
//!       http: auto
//!       ws: auto
//!       admin: 8020  # fixed port
//!     env:
//!       DATABASE_URL: "postgres://localhost:{postgres.port.main}/graph"
//! ```

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::atomic::{AtomicU16, Ordering};

/// Port specification - either auto-allocated or fixed.
#[derive(Debug, Clone, PartialEq)]
pub enum PortSpec {
    /// Automatically allocate an available port from the configured range.
    Auto,
    /// Use a specific fixed port.
    Fixed(u16),
}

impl Default for PortSpec {
    fn default() -> Self {
        PortSpec::Auto
    }
}

// Custom serialization for PortSpec
impl Serialize for PortSpec {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            PortSpec::Auto => serializer.serialize_str("auto"),
            PortSpec::Fixed(port) => serializer.serialize_u16(*port),
        }
    }
}

impl<'de> Deserialize<'de> for PortSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{self, Visitor};

        struct PortSpecVisitor;

        impl<'de> Visitor<'de> for PortSpecVisitor {
            type Value = PortSpec;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("'auto' or a port number")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if v == "auto" {
                    Ok(PortSpec::Auto)
                } else {
                    // Try to parse as number
                    v.parse::<u16>()
                        .map(PortSpec::Fixed)
                        .map_err(|_| de::Error::custom(format!("expected 'auto' or port number, got '{}'", v)))
                }
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if v > u16::MAX as u64 {
                    Err(de::Error::custom(format!("port {} out of range", v)))
                } else {
                    Ok(PortSpec::Fixed(v as u16))
                }
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if v < 0 || v > u16::MAX as i64 {
                    Err(de::Error::custom(format!("port {} out of range", v)))
                } else {
                    Ok(PortSpec::Fixed(v as u16))
                }
            }
        }

        deserializer.deserialize_any(PortSpecVisitor)
    }
}

/// Named port configuration for a service.
///
/// Maps port names to their specifications.
pub type PortConfig = HashMap<String, PortSpec>;

/// Registry of allocated ports.
///
/// Maps service_name -> port_name -> allocated_port
pub type PortRegistry = HashMap<String, HashMap<String, u16>>;

/// Error type for port allocation.
#[derive(thiserror::Error, Debug)]
pub enum PortError {
    /// No available ports in the configured range.
    #[error("No available ports in range {0}-{1}")]
    NoAvailablePorts(u16, u16),

    /// Port is already in use.
    #[error("Port {0} is already in use")]
    PortInUse(u16),

    /// Invalid port reference.
    #[error("Invalid port reference: {0}")]
    InvalidReference(String),

    /// Service not found in registry.
    #[error("Service '{0}' not found in port registry")]
    ServiceNotFound(String),

    /// Port not found for service.
    #[error("Port '{1}' not found for service '{0}'")]
    PortNotFound(String, String),

    /// Unresolved placeholder in template.
    #[error("Unresolved placeholder in template: {0}")]
    UnresolvedPlaceholder(String),
}

/// Port allocator that manages dynamic port assignment.
///
/// The allocator maintains a range of ports and tracks which ones have been
/// allocated. It uses atomic operations to ensure thread-safety.
pub struct PortAllocator {
    /// Start of the allocation range (inclusive).
    range_start: u16,
    /// End of the allocation range (inclusive).
    range_end: u16,
    /// Next port to try allocating.
    next_port: AtomicU16,
    /// Registry of allocated ports (service -> port_name -> port).
    /// Uses internal mutability via the registry being built up.
    registry: PortRegistry,
}

impl PortAllocator {
    /// Create a new port allocator with the given range.
    ///
    /// The range is inclusive on both ends.
    pub fn new(range_start: u16, range_end: u16) -> Self {
        Self {
            range_start,
            range_end,
            next_port: AtomicU16::new(range_start),
            registry: HashMap::new(),
        }
    }

    /// Create a new port allocator with the default range (49152-65535).
    ///
    /// This uses the IANA ephemeral port range.
    pub fn with_default_range() -> Self {
        Self::new(49152, 65535)
    }

    /// Allocate all ports for a service based on its port configuration.
    ///
    /// Returns the map of port names to allocated ports.
    pub fn allocate_for_service(
        &mut self,
        service_name: &str,
        config: &PortConfig,
    ) -> Result<HashMap<String, u16>, PortError> {
        let mut allocated = HashMap::new();

        for (port_name, spec) in config {
            let port = match spec {
                PortSpec::Auto => self.allocate_auto()?,
                PortSpec::Fixed(p) => {
                    self.verify_available(*p)?;
                    *p
                }
            };
            allocated.insert(port_name.clone(), port);
        }

        // Store in registry
        self.registry.insert(service_name.to_string(), allocated.clone());

        Ok(allocated)
    }

    /// Allocate a single auto port.
    fn allocate_auto(&self) -> Result<u16, PortError> {
        let mut attempts = 0;
        let max_attempts = (self.range_end - self.range_start) as usize + 1;

        while attempts < max_attempts {
            let port = self.next_port.fetch_add(1, Ordering::SeqCst);

            // Wrap around if we exceed the range
            if port > self.range_end {
                self.next_port.store(self.range_start, Ordering::SeqCst);
                attempts += 1;
                continue;
            }

            // Check if port is available by trying to bind
            if Self::is_port_available(port) {
                return Ok(port);
            }

            attempts += 1;
        }

        Err(PortError::NoAvailablePorts(self.range_start, self.range_end))
    }

    /// Verify a fixed port is available.
    fn verify_available(&self, port: u16) -> Result<(), PortError> {
        if Self::is_port_available(port) {
            Ok(())
        } else {
            Err(PortError::PortInUse(port))
        }
    }

    /// Check if a port is available by attempting to bind to it.
    fn is_port_available(port: u16) -> bool {
        TcpListener::bind(("127.0.0.1", port)).is_ok()
    }

    /// Get the port registry.
    pub fn registry(&self) -> &PortRegistry {
        &self.registry
    }

    /// Get an allocated port by service and port name.
    pub fn get_port(&self, service: &str, port_name: &str) -> Option<u16> {
        self.registry
            .get(service)
            .and_then(|ports| ports.get(port_name))
            .copied()
    }

    /// Resolve a port reference string.
    ///
    /// Supports two formats:
    /// - `{port.name}` - resolves to own port (requires current_service)
    /// - `{service.port.name}` - resolves to another service's port
    ///
    /// Returns the resolved port number or an error.
    pub fn resolve_reference(
        &self,
        reference: &str,
        current_service: Option<&str>,
    ) -> Result<u16, PortError> {
        // Strip braces if present
        let inner = reference
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .unwrap_or(reference);

        let parts: Vec<&str> = inner.split('.').collect();

        match parts.as_slice() {
            // {port.name} - own port
            ["port", port_name] => {
                let service = current_service.ok_or_else(|| {
                    PortError::InvalidReference(format!(
                        "Cannot use {{port.{}}} without a current service context",
                        port_name
                    ))
                })?;
                self.get_port(service, port_name).ok_or_else(|| {
                    PortError::PortNotFound(service.to_string(), port_name.to_string())
                })
            }
            // {service.port.name} - another service's port
            [service, "port", port_name] => self.get_port(service, port_name).ok_or_else(|| {
                PortError::PortNotFound(service.to_string(), port_name.to_string())
            }),
            _ => Err(PortError::InvalidReference(reference.to_string())),
        }
    }

    /// Substitute all port references in a string.
    ///
    /// Replaces patterns like `{port.http}` and `{postgres.port.main}` with
    /// their allocated port numbers.
    pub fn substitute_ports(
        &self,
        template: &str,
        current_service: Option<&str>,
    ) -> Result<String, PortError> {
        let mut result = template.to_string();

        // Find all {xxx.port.yyy} or {port.yyy} patterns
        let re_service_port = regex::Regex::new(r"\{(\w+)\.port\.(\w+)\}").unwrap();
        let re_own_port = regex::Regex::new(r"\{port\.(\w+)\}").unwrap();

        // Replace {service.port.name} patterns
        for cap in re_service_port.captures_iter(template) {
            let full_match = cap.get(0).unwrap().as_str();
            let service = cap.get(1).unwrap().as_str();
            let port_name = cap.get(2).unwrap().as_str();

            let port = self.get_port(service, port_name).ok_or_else(|| {
                PortError::PortNotFound(service.to_string(), port_name.to_string())
            })?;

            result = result.replace(full_match, &port.to_string());
        }

        // Replace {port.name} patterns (own ports)
        if let Some(service) = current_service {
            for cap in re_own_port.captures_iter(template) {
                let full_match = cap.get(0).unwrap().as_str();
                let port_name = cap.get(1).unwrap().as_str();

                let port = self.get_port(service, port_name).ok_or_else(|| {
                    PortError::PortNotFound(service.to_string(), port_name.to_string())
                })?;

                result = result.replace(full_match, &port.to_string());
            }
        }

        // Note: We don't check for remaining {placeholders} here because
        // other substitution systems (like params) may handle them.
        // Template file processing should do a final validation.

        Ok(result)
    }
}

impl Default for PortAllocator {
    fn default() -> Self {
        Self::with_default_range()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_port_spec_serde() {
        // Test Auto
        let auto = PortSpec::Auto;
        let yaml = serde_yaml::to_string(&auto).unwrap();
        assert_eq!(yaml.trim(), "auto");

        let parsed: PortSpec = serde_yaml::from_str("auto").unwrap();
        assert_eq!(parsed, PortSpec::Auto);

        // Test Fixed
        let fixed = PortSpec::Fixed(8080);
        let yaml = serde_yaml::to_string(&fixed).unwrap();
        assert_eq!(yaml.trim(), "8080");

        let parsed: PortSpec = serde_yaml::from_str("8080").unwrap();
        assert_eq!(parsed, PortSpec::Fixed(8080));
    }

    #[test]
    fn test_port_config_serde() {
        let yaml = r#"
http: auto
ws: auto
admin: 8020
"#;

        let config: PortConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.get("http"), Some(&PortSpec::Auto));
        assert_eq!(config.get("ws"), Some(&PortSpec::Auto));
        assert_eq!(config.get("admin"), Some(&PortSpec::Fixed(8020)));
    }

    #[test]
    fn test_allocator_auto_ports() {
        let mut allocator = PortAllocator::new(50000, 50100);

        let config: PortConfig = [
            ("http".to_string(), PortSpec::Auto),
            ("ws".to_string(), PortSpec::Auto),
        ]
        .into_iter()
        .collect();

        let ports = allocator.allocate_for_service("test-service", &config).unwrap();

        assert!(ports.contains_key("http"));
        assert!(ports.contains_key("ws"));
        assert_ne!(ports.get("http"), ports.get("ws"));

        // Verify they're in range
        let http = *ports.get("http").unwrap();
        let ws = *ports.get("ws").unwrap();
        assert!(http >= 50000 && http <= 50100);
        assert!(ws >= 50000 && ws <= 50100);
    }

    #[test]
    fn test_allocator_fixed_ports() {
        let mut allocator = PortAllocator::new(50000, 50100);

        // Use ports in ephemeral range that are unlikely to be in use
        let config: PortConfig = [
            ("http".to_string(), PortSpec::Fixed(59080)),
            ("admin".to_string(), PortSpec::Fixed(59020)),
        ]
        .into_iter()
        .collect();

        let ports = allocator.allocate_for_service("test-service", &config).unwrap();

        assert_eq!(ports.get("http"), Some(&59080));
        assert_eq!(ports.get("admin"), Some(&59020));
    }

    #[test]
    fn test_port_resolution() {
        let mut allocator = PortAllocator::new(50000, 50100);

        // Allocate ports for postgres
        let postgres_config: PortConfig = [("main".to_string(), PortSpec::Fixed(5432))]
            .into_iter()
            .collect();
        allocator.allocate_for_service("postgres", &postgres_config).unwrap();

        // Allocate ports for graph-node
        let gn_config: PortConfig = [
            ("http".to_string(), PortSpec::Auto),
            ("ws".to_string(), PortSpec::Auto),
        ]
        .into_iter()
        .collect();
        allocator.allocate_for_service("graph-node", &gn_config).unwrap();

        // Test resolution
        let port = allocator.resolve_reference("{postgres.port.main}", None).unwrap();
        assert_eq!(port, 5432);

        let port = allocator.resolve_reference("{port.http}", Some("graph-node")).unwrap();
        assert!(port >= 50000 && port <= 50100);
    }

    #[test]
    fn test_port_substitution() {
        let mut allocator = PortAllocator::new(50000, 50100);

        // Allocate ports
        let postgres_config: PortConfig = [("main".to_string(), PortSpec::Fixed(5432))]
            .into_iter()
            .collect();
        allocator.allocate_for_service("postgres", &postgres_config).unwrap();

        let gn_config: PortConfig = [("http".to_string(), PortSpec::Fixed(8000))]
            .into_iter()
            .collect();
        allocator.allocate_for_service("graph-node", &gn_config).unwrap();

        // Test substitution
        let template = "postgres://localhost:{postgres.port.main}/graph";
        let result = allocator.substitute_ports(template, Some("graph-node")).unwrap();
        assert_eq!(result, "postgres://localhost:5432/graph");

        let template = "http://localhost:{port.http}/graphql";
        let result = allocator.substitute_ports(template, Some("graph-node")).unwrap();
        assert_eq!(result, "http://localhost:8000/graphql");
    }

    #[test]
    fn test_registry_access() {
        let mut allocator = PortAllocator::new(50000, 50100);

        let config: PortConfig = [
            ("http".to_string(), PortSpec::Fixed(8000)),
            ("ws".to_string(), PortSpec::Fixed(8001)),
        ]
        .into_iter()
        .collect();

        allocator.allocate_for_service("my-service", &config).unwrap();

        assert_eq!(allocator.get_port("my-service", "http"), Some(8000));
        assert_eq!(allocator.get_port("my-service", "ws"), Some(8001));
        assert_eq!(allocator.get_port("my-service", "nonexistent"), None);
        assert_eq!(allocator.get_port("other-service", "http"), None);
    }
}
