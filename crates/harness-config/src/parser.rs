//! Configuration parser with environment variable substitution

use crate::{
    Config, ConfigError, HealthCheck, HealthCheckType, PortMapping, Result, Service, ServiceType,
    resolver::{ResolutionContext, resolve_service_env, validate_references},
};
use regex::Regex;
use service_orchestration::{HealthCheck as OrchestratorHealthCheck, ServiceConfig, ServiceTarget};
use std::collections::HashMap;
use std::path::Path;

/// Parse a YAML configuration file
pub fn parse_file(path: impl AsRef<Path>) -> Result<Config> {
    let content = std::fs::read_to_string(path)?;
    parse_str(&content)
}

/// Parse YAML configuration from a string
pub fn parse_str(content: &str) -> Result<Config> {
    let config: Config = serde_yaml::from_str(content)?;
    validate_config(&config)?;
    Ok(config)
}

/// Validate configuration
fn validate_config(config: &Config) -> Result<()> {
    // Check version
    if config.version != "1.0" {
        return Err(ConfigError::ValidationError(format!(
            "Unsupported version: {}, expected 1.0",
            config.version
        )));
    }

    // Check all service network references exist
    for (name, service) in &config.services {
        if !config.networks.contains_key(&service.network) {
            return Err(ConfigError::ValidationError(format!(
                "Service '{}' references unknown network '{}'",
                name, service.network
            )));
        }

        // Check dependencies exist
        for dep in &service.dependencies {
            if !config.services.contains_key(dep) {
                return Err(ConfigError::ValidationError(format!(
                    "Service '{}' depends on unknown service '{}'",
                    name, dep
                )));
            }
        }
    }

    // Validate all variable references
    validate_references(config)?;

    Ok(())
}

/// Substitute environment variables in a string
pub fn substitute_env_vars(input: &str) -> Result<String> {
    let re = Regex::new(r"\$\{([^}]+)\}").unwrap();
    let mut result = input.to_string();
    let mut errors = Vec::new();

    for cap in re.captures_iter(input) {
        let full_match = &cap[0];
        let var_expr = &cap[1];

        // Handle default values: ${VAR:-default}
        let (var_name, default_value) = if let Some(pos) = var_expr.find(":-") {
            let name = &var_expr[..pos];
            let default = &var_expr[pos + 2..];
            (name, Some(default))
        } else {
            (var_expr, None)
        };

        // Get value from environment or default
        match std::env::var(var_name) {
            Ok(value) => {
                result = result.replace(full_match, &value);
            }
            Err(_) => {
                if let Some(default) = default_value {
                    result = result.replace(full_match, default);
                } else {
                    errors.push(var_name.to_string());
                }
            }
        }
    }

    if !errors.is_empty() {
        return Err(ConfigError::EnvVarNotFound(errors.join(", ")));
    }

    Ok(result)
}

/// Substitute service references (${service.ip}) with actual values
pub fn substitute_service_refs(
    input: &str,
    service_ips: &HashMap<String, String>,
) -> Result<String> {
    let re = Regex::new(r"\$\{([^}]+)\.ip\}").unwrap();
    let mut result = input.to_string();

    for cap in re.captures_iter(input) {
        let full_match = &cap[0];
        let service_name = &cap[1];

        if let Some(ip) = service_ips.get(service_name) {
            result = result.replace(full_match, ip);
        } else {
            return Err(ConfigError::ServiceNotFound(service_name.to_string()));
        }
    }

    Ok(result)
}

/// Process all environment variables in a service
pub fn process_service_env(
    service: &Service,
    service_ips: &HashMap<String, String>,
) -> Result<HashMap<String, String>> {
    let mut processed_env = HashMap::new();

    for (key, value) in &service.env {
        // First substitute environment variables
        let env_substituted = substitute_env_vars(value)?;
        // Then substitute service references
        let fully_substituted = substitute_service_refs(&env_substituted, service_ips)?;
        processed_env.insert(key.clone(), fully_substituted);
    }

    Ok(processed_env)
}

/// Convert configuration to orchestrator types
pub fn convert_to_orchestrator(config: &Config, service_name: &str) -> Result<ServiceConfig> {
    convert_to_orchestrator_with_context(config, service_name, None)
}

/// Convert configuration to orchestrator types with resolution context
pub fn convert_to_orchestrator_with_context(
    config: &Config,
    service_name: &str,
    context: Option<&ResolutionContext>,
) -> Result<ServiceConfig> {
    let service = config
        .services
        .get(service_name)
        .ok_or_else(|| ConfigError::ServiceNotFound(service_name.to_string()))?;

    // Use provided context or create a default one
    let default_context = ResolutionContext::new();
    let ctx = context.unwrap_or(&default_context);

    let env = resolve_service_env(service, ctx)?;

    let target = match &service.service_type {
        ServiceType::Docker {
            image,
            ports,
            volumes,
            ..
        } => {
            // Convert port mappings to simple u16 for MVP
            let simple_ports: Vec<u16> = ports
                .iter()
                .filter_map(|p| match p {
                    PortMapping::Simple(port) => Some(*port),
                    PortMapping::Full(_) => None, // Skip complex mappings for MVP
                })
                .collect();

            ServiceTarget::Docker {
                image: image.clone(),
                env,
                ports: simple_ports,
                volumes: volumes.clone(),
            }
        }

        ServiceType::Process {
            binary,
            args,
            working_dir,
            ..
        } => ServiceTarget::Process {
            binary: binary.clone(),
            args: args.clone(),
            env,
            working_dir: working_dir.clone(),
        },

        ServiceType::Remote {
            host,
            binary,
            args,
            working_dir,
        } => {
            // For MVP, assume LAN network for remote services
            // In Phase 6, we'll properly handle network types
            ServiceTarget::RemoteLan {
                host: host.clone(),
                user: "root".to_string(), // Default for MVP
                binary: binary.clone(),
                args: args.clone(),
            }
        }

        ServiceType::Package { host, package, .. } => {
            // Map to WireGuard target for MVP
            ServiceTarget::Wireguard {
                host: host.clone(),
                user: "root".to_string(), // Default for MVP
                package: package.clone(),
            }
        }
    };

    let health_check = service
        .health_check
        .as_ref()
        .map(|hc| convert_health_check(hc));

    Ok(ServiceConfig {
        name: service_name.to_string(),
        target,
        dependencies: service
            .dependencies
            .iter()
            .map(|dep| service_orchestration::Dependency::Service {
                service: dep.clone(),
            })
            .collect(),
        health_check,
    })
}

/// Convert health check configuration
fn convert_health_check(hc: &HealthCheck) -> OrchestratorHealthCheck {
    let (command, args) = match &hc.check_type {
        HealthCheckType::Command { command, args } => (command.clone(), args.clone()),
        HealthCheckType::Http { http } => {
            // Convert HTTP check to curl command for MVP
            ("curl".to_string(), vec!["-f".to_string(), http.clone()])
        }
        HealthCheckType::Tcp { tcp } => {
            // Convert TCP check to nc command for MVP
            (
                "nc".to_string(),
                vec![
                    "-z".to_string(),
                    "localhost".to_string(),
                    tcp.port.to_string(),
                ],
            )
        }
    };

    OrchestratorHealthCheck {
        command,
        args,
        interval: hc.interval,
        retries: hc.retries,
        timeout: hc.timeout,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_var_substitution() {
        // Use an existing environment variable that's likely to be set
        if let Ok(home) = std::env::var("HOME") {
            let result = substitute_env_vars("${HOME}").unwrap();
            assert_eq!(result, home);

            let result = substitute_env_vars("prefix-${HOME}-suffix").unwrap();
            assert_eq!(result, format!("prefix-{}-suffix", home));
        } else if let Ok(user) = std::env::var("USER") {
            let result = substitute_env_vars("${USER}").unwrap();
            assert_eq!(result, user);

            let result = substitute_env_vars("prefix-${USER}-suffix").unwrap();
            assert_eq!(result, format!("prefix-{}-suffix", user));
        } else {
            // Skip test if no suitable env var is available
            println!("Skipping test - no suitable environment variable found");
        }
    }

    #[test]
    fn test_env_var_with_default() {
        // Test with a variable that's unlikely to exist
        let result =
            substitute_env_vars("${MISSING_VAR_UNLIKELY_TO_EXIST:-default_value}").unwrap();
        assert_eq!(result, "default_value");

        // Test with an existing variable (use one that's likely to be set)
        if let Ok(path) = std::env::var("PATH") {
            let result = substitute_env_vars("${PATH:-default}").unwrap();
            assert_eq!(result, path);
        } else {
            // If PATH isn't set, just test the default case
            let result = substitute_env_vars("${ANOTHER_MISSING_VAR:-default}").unwrap();
            assert_eq!(result, "default");
        }
    }

    #[test]
    fn test_service_ref_substitution() {
        let mut service_ips = HashMap::new();
        service_ips.insert("postgres".to_string(), "192.168.1.10".to_string());

        let result = substitute_service_refs(
            "postgresql://user:pass@${postgres.ip}:5432/db",
            &service_ips,
        )
        .unwrap();

        assert_eq!(result, "postgresql://user:pass@192.168.1.10:5432/db");
    }

    #[test]
    fn test_parse_minimal_config() {
        let yaml = r#"
version: "1.0"
networks:
  local:
    type: local
services:
  test:
    type: process
    network: local
    binary: "/usr/bin/echo"
    args: ["hello"]
"#;

        let config = parse_str(yaml).unwrap();
        assert_eq!(config.version, "1.0");
        assert_eq!(config.services.len(), 1);
        assert!(config.services.contains_key("test"));
    }
}
