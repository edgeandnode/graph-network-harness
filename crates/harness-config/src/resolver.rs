//! Environment variable and service reference resolver
//!
//! This module handles resolution of:
//! - Environment variables: ${VAR} and ${VAR:-default}
//! - Service references: ${service.ip}, ${service.port}, ${service.host}

use crate::{Config, ConfigError, Result, Service};
use regex::Regex;
use std::collections::{HashMap, HashSet};

/// Context for resolving variables and references
#[derive(Debug, Clone)]
pub struct ResolutionContext {
    /// Environment variables (can be overridden)
    pub env_vars: HashMap<String, String>,
    /// Service IP addresses
    pub service_ips: HashMap<String, String>,
    /// Service ports (first exposed port)
    pub service_ports: HashMap<String, u16>,
    /// Service hostnames
    pub service_hosts: HashMap<String, String>,
}

impl ResolutionContext {
    /// Create a new resolution context
    pub fn new() -> Self {
        Self {
            env_vars: std::env::vars().collect(),
            service_ips: HashMap::new(),
            service_ports: HashMap::new(),
            service_hosts: HashMap::new(),
        }
    }

    /// Add or update an environment variable
    pub fn set_env(&mut self, key: String, value: String) {
        self.env_vars.insert(key, value);
    }

    /// Add service network information
    pub fn add_service(&mut self, name: String, ip: String, port: Option<u16>, host: String) {
        self.service_ips.insert(name.clone(), ip);
        self.service_hosts.insert(name.clone(), host);
        if let Some(p) = port {
            self.service_ports.insert(name, p);
        }
    }
}

/// Resolve all variables in a string
pub fn resolve_string(input: &str, context: &ResolutionContext) -> Result<String> {
    let mut result = input.to_string();
    
    // First resolve environment variables
    result = resolve_env_vars(&result, &context.env_vars)?;
    
    // Then resolve service references
    result = resolve_service_refs(&result, context)?;
    
    Ok(result)
}

/// Resolve environment variables in a string
fn resolve_env_vars(input: &str, env_vars: &HashMap<String, String>) -> Result<String> {
    let re = Regex::new(r"\$\{([^}]+)\}").unwrap();
    let mut result = input.to_string();
    let mut errors = Vec::new();

    for cap in re.captures_iter(input) {
        let full_match = &cap[0];
        let var_expr = &cap[1];

        // Skip service references (they have dots)
        if var_expr.contains('.') {
            continue;
        }

        // Handle default values: ${VAR:-default}
        let (var_name, default_value) = if let Some(pos) = var_expr.find(":-") {
            let name = &var_expr[..pos];
            let default = &var_expr[pos + 2..];
            (name, Some(default))
        } else {
            (var_expr, None)
        };

        // Get value from context or environment
        if let Some(value) = env_vars.get(var_name) {
            result = result.replace(full_match, value);
        } else if let Ok(value) = std::env::var(var_name) {
            result = result.replace(full_match, &value);
        } else if let Some(default) = default_value {
            result = result.replace(full_match, default);
        } else {
            errors.push(var_name.to_string());
        }
    }

    if !errors.is_empty() {
        return Err(ConfigError::EnvVarNotFound(errors.join(", ")));
    }

    Ok(result)
}

/// Resolve service references in a string
fn resolve_service_refs(input: &str, context: &ResolutionContext) -> Result<String> {
    let re = Regex::new(r"\$\{([^}]+)\.(ip|port|host)\}").unwrap();
    let mut result = input.to_string();
    let mut errors = Vec::new();

    for cap in re.captures_iter(input) {
        let full_match = &cap[0];
        let service_name = &cap[1];
        let ref_type = &cap[2];

        let replacement_made = match ref_type {
            "ip" => {
                if let Some(ip) = context.service_ips.get(service_name) {
                    result = result.replace(full_match, ip);
                    true
                } else {
                    false
                }
            }
            "host" => {
                if let Some(host) = context.service_hosts.get(service_name) {
                    result = result.replace(full_match, host);
                    true
                } else {
                    false
                }
            }
            "port" => {
                if let Some(port) = context.service_ports.get(service_name) {
                    result = result.replace(full_match, &port.to_string());
                    true
                } else {
                    false
                }
            }
            _ => false,
        };

        if !replacement_made {
            errors.push(format!("{}.{}", service_name, ref_type));
        }
    }

    if !errors.is_empty() {
        return Err(ConfigError::ServiceNotFound(errors.join(", ")));
    }

    Ok(result)
}

/// Resolve all environment variables in a service configuration
pub fn resolve_service_env(
    service: &Service,
    context: &ResolutionContext,
) -> Result<HashMap<String, String>> {
    let mut resolved_env = HashMap::new();

    for (key, value) in &service.env {
        let resolved_value = resolve_string(value, context)?;
        resolved_env.insert(key.clone(), resolved_value);
    }

    Ok(resolved_env)
}

/// Find all variable references in a configuration
pub fn find_all_references(config: &Config) -> (HashSet<String>, HashSet<String>) {
    let mut env_vars = HashSet::new();
    let mut service_refs = HashSet::new();
    
    let env_re = Regex::new(r"\$\{([^}]+)\}").unwrap();
    let service_re = Regex::new(r"\$\{([^}]+)\.(ip|port|host)\}").unwrap();

    // Check all service environment variables
    for service in config.services.values() {
        for value in service.env.values() {
            // Find environment variables
            for cap in env_re.captures_iter(value) {
                let var_expr = &cap[1];
                if !var_expr.contains('.') {
                    let var_name = if let Some(pos) = var_expr.find(":-") {
                        &var_expr[..pos]
                    } else {
                        var_expr
                    };
                    env_vars.insert(var_name.to_string());
                }
            }
            
            // Find service references
            for cap in service_re.captures_iter(value) {
                let service_ref = format!("{}.{}", &cap[1], &cap[2]);
                service_refs.insert(service_ref);
            }
        }
    }

    (env_vars, service_refs)
}

/// Validate that all references can be resolved
pub fn validate_references(config: &Config) -> Result<()> {
    let (env_vars, service_refs) = find_all_references(config);
    
    // Check service references
    for service_ref in &service_refs {
        if let Some(dot_pos) = service_ref.find('.') {
            let service_name = &service_ref[..dot_pos];
            let ref_type = &service_ref[dot_pos + 1..];
            
            if !config.services.contains_key(service_name) {
                return Err(ConfigError::ValidationError(format!(
                    "Service reference '{}' refers to unknown service",
                    service_ref
                )));
            }
            
            if !["ip", "port", "host"].contains(&ref_type) {
                return Err(ConfigError::ValidationError(format!(
                    "Invalid service reference type '{}' in '{}'",
                    ref_type, service_ref
                )));
            }
        }
    }
    
    // For environment variables, we can only warn since they might be set at runtime
    let missing_env_vars: Vec<String> = env_vars
        .into_iter()
        .filter(|var| std::env::var(var).is_err())
        .collect();
        
    if !missing_env_vars.is_empty() {
        eprintln!(
            "Warning: The following environment variables are not set: {}",
            missing_env_vars.join(", ")
        );
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_env_vars() {
        let mut env_vars = HashMap::new();
        env_vars.insert("TEST_VAR".to_string(), "test_value".to_string());
        env_vars.insert("PORT".to_string(), "8080".to_string());

        let result = resolve_env_vars("${TEST_VAR}", &env_vars).unwrap();
        assert_eq!(result, "test_value");

        let result = resolve_env_vars("http://localhost:${PORT}", &env_vars).unwrap();
        assert_eq!(result, "http://localhost:8080");

        let result = resolve_env_vars("${MISSING:-default}", &env_vars).unwrap();
        assert_eq!(result, "default");

        let result = resolve_env_vars("${TEST_VAR:-ignored}", &env_vars).unwrap();
        assert_eq!(result, "test_value");
    }

    #[test]
    fn test_resolve_service_refs() {
        let mut context = ResolutionContext::new();
        context.add_service(
            "postgres".to_string(),
            "192.168.1.10".to_string(),
            Some(5432),
            "postgres.local".to_string(),
        );

        let result = resolve_service_refs("${postgres.ip}", &context).unwrap();
        assert_eq!(result, "192.168.1.10");

        let result = resolve_service_refs("${postgres.port}", &context).unwrap();
        assert_eq!(result, "5432");

        let result = resolve_service_refs("${postgres.host}", &context).unwrap();
        assert_eq!(result, "postgres.local");

        let result = resolve_service_refs(
            "postgresql://user:pass@${postgres.ip}:${postgres.port}/db",
            &context,
        )
        .unwrap();
        assert_eq!(result, "postgresql://user:pass@192.168.1.10:5432/db");
    }

    #[test]
    fn test_resolve_string_combined() {
        let mut context = ResolutionContext::new();
        context.set_env("DB_USER".to_string(), "admin".to_string());
        context.add_service(
            "postgres".to_string(),
            "192.168.1.10".to_string(),
            Some(5432),
            "postgres.local".to_string(),
        );

        let result = resolve_string(
            "postgresql://${DB_USER}:${DB_PASS:-secret}@${postgres.ip}:${postgres.port}/db",
            &context,
        )
        .unwrap();

        assert_eq!(result, "postgresql://admin:secret@192.168.1.10:5432/db");
    }

    #[test]
    fn test_find_all_references() {
        let yaml = r#"
version: "1.0"
networks:
  local:
    type: local
services:
  postgres:
    type: docker
    network: local
    image: postgres
  api:
    type: process
    network: local
    binary: api-server
    env:
      DATABASE_URL: "postgresql://${DB_USER}:${DB_PASS:-secret}@${postgres.ip}:5432/db"
      API_PORT: "${PORT:-8080}"
      API_HOST: "${api.host}"
"#;

        let config = crate::parser::parse_str(yaml).unwrap();
        let (env_vars, service_refs) = find_all_references(&config);

        assert!(env_vars.contains("DB_USER"));
        assert!(env_vars.contains("DB_PASS"));
        assert!(env_vars.contains("PORT"));
        assert!(service_refs.contains("postgres.ip"));
        assert!(service_refs.contains("api.host"));
    }
}