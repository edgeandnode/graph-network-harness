use crate::commands::dependencies;
use anyhow::{Context, Result};
use harness_config::{HealthCheckType, ServiceType, parser, resolver};
use std::collections::HashMap;
use std::path::Path;

pub async fn run(config_path: &Path, strict: bool) -> Result<()> {
    println!("Validating {}...", config_path.display());

    // Try to parse the configuration
    let config = parser::parse_file(config_path).context("Failed to parse configuration")?;

    // Basic validation is done during parsing
    println!("✓ Configuration syntax valid");
    println!("  Version: {}", config.version);

    if let Some(name) = &config.name {
        println!("  Name: {name}");
    }

    println!("  Networks: {}", config.networks.len());
    println!("  Services: {}", config.services.len());

    let mut validation_errors = Vec::new();
    let mut warnings = Vec::new();

    // Check for circular dependencies
    println!("\n🔍 Checking service dependencies...");
    match dependencies::topological_sort(&config, &[]) {
        Ok(sorted) => {
            println!("  ✓ No circular dependencies found");
            println!("  ✓ Service start order: {}", sorted.join(" → "));
        }
        Err(e) => {
            validation_errors.push(format!("Circular dependency detected: {e}"));
        }
    }

    // Check for port conflicts
    println!("\n🔍 Checking for port conflicts...");
    let mut port_usage: HashMap<u16, Vec<String>> = HashMap::new();

    for (name, service) in &config.services {
        if let ServiceType::Docker { ports, .. } = &service.service_type {
            for port_mapping in ports {
                let port = match port_mapping {
                    harness_config::PortMapping::Simple(p) => *p,
                    harness_config::PortMapping::Full(mapping) => {
                        // Parse host:container format
                        if let Some((host_port, _)) = mapping.split_once(':') {
                            host_port.parse::<u16>().unwrap_or(0)
                        } else {
                            0
                        }
                    }
                };

                if port > 0 {
                    port_usage.entry(port).or_default().push(name.clone());
                }
            }
        }
    }

    for (port, services) in &port_usage {
        if services.len() > 1 {
            validation_errors.push(format!(
                "Port {} is used by multiple services: {}",
                port,
                services.join(", ")
            ));
        }
    }

    if port_usage.is_empty() {
        println!("  ✓ No port mappings found");
    } else {
        let conflicts = port_usage.values().filter(|s| s.len() > 1).count();
        if conflicts == 0 {
            println!("  ✓ No port conflicts detected");
        }
    }

    // Validate health checks
    println!("\n🔍 Validating health checks...");
    let mut health_check_count = 0;

    for (name, service) in &config.services {
        if let Some(health_check) = &service.health_check {
            health_check_count += 1;

            match &health_check.check_type {
                HealthCheckType::Http { http } => {
                    // Validate HTTP URL format
                    if !http.starts_with("http://") && !http.starts_with("https://") {
                        warnings.push(format!(
                            "Service '{name}' health check URL should start with http:// or https://"
                        ));
                    }
                }
                HealthCheckType::Tcp { tcp } => {
                    if tcp.port == 0 {
                        validation_errors.push(format!(
                            "Service '{name}' has invalid TCP health check port: 0"
                        ));
                    }
                }
                HealthCheckType::Command { command, .. } => {
                    if command.is_empty() {
                        validation_errors
                            .push(format!("Service '{name}' has empty health check command"));
                    }
                }
            }

            // Check health check parameters
            if health_check.interval == 0 {
                warnings.push(format!("Service '{name}' has health check interval of 0"));
            }
            if health_check.timeout >= health_check.interval {
                warnings.push(format!(
                    "Service '{}' health check timeout ({}) should be less than interval ({})",
                    name, health_check.timeout, health_check.interval
                ));
            }
        }
    }

    if health_check_count == 0 {
        println!("  ⚠ No health checks configured");
    } else {
        println!("  ✓ {health_check_count} health checks configured");
    }

    // Check environment variables and service references
    println!("\n🔍 Checking variable references...");
    let (env_vars, service_refs) = resolver::find_all_references(&config)?;

    // Check service references
    let mut invalid_refs = Vec::new();
    for service_ref in &service_refs {
        if let Some(dot_pos) = service_ref.find('.') {
            let service_name = &service_ref[..dot_pos];
            let ref_type = &service_ref[dot_pos + 1..];

            if !config.services.contains_key(service_name) {
                invalid_refs.push(service_ref.clone());
            } else if ref_type == "port" {
                // Check if the referenced service actually exposes ports
                if let Some(service) = config.services.get(service_name) {
                    match &service.service_type {
                        ServiceType::Docker { ports, .. } => {
                            if ports.is_empty() {
                                warnings.push(format!(
                                    "Reference '{service_ref}' used but service has no ports configured"
                                ));
                            }
                        }
                        _ => {
                            warnings.push(format!(
                                "Reference '{service_ref}' used but service type doesn't expose ports"
                            ));
                        }
                    }
                }
            }
        }
    }

    if !invalid_refs.is_empty() {
        validation_errors.push(format!(
            "Invalid service references: {}",
            invalid_refs.join(", ")
        ));
    }

    // Check environment variables
    let missing_env_vars: Vec<String> = env_vars
        .into_iter()
        .filter(|var| std::env::var(var).is_err())
        .collect();

    if !missing_env_vars.is_empty() {
        if strict {
            validation_errors.push(format!(
                "Missing environment variables: {}",
                missing_env_vars.join(", ")
            ));
        } else {
            warnings.push(format!(
                "Environment variables not currently set: {}",
                missing_env_vars.join(", ")
            ));
        }
    }

    println!("  ✓ {} service references found", service_refs.len());

    // Check network configuration
    println!("\n🔍 Checking network configuration...");
    let mut services_per_network: HashMap<String, Vec<String>> = HashMap::new();

    for (name, service) in &config.services {
        services_per_network
            .entry(service.network.clone())
            .or_default()
            .push(name.clone());
    }

    for (network, services) in &services_per_network {
        if services.len() == 1 && config.networks.len() > 1 {
            warnings.push(format!(
                "Network '{}' has only one service ({}), consider consolidating networks",
                network, services[0]
            ));
        }
    }

    // Check for unused networks
    for network_name in config.networks.keys() {
        if !services_per_network.contains_key(network_name) {
            warnings.push(format!(
                "Network '{network_name}' is defined but not used by any service"
            ));
        }
    }

    println!("  ✓ Network configuration validated");

    // Print summary
    println!("\n📊 Validation Summary:");

    if validation_errors.is_empty() && warnings.is_empty() {
        println!("  ✅ All validation checks passed!");
    } else {
        if !validation_errors.is_empty() {
            println!("\n❌ Errors ({}):", validation_errors.len());
            for error in &validation_errors {
                println!("  - {error}");
            }
        }

        if !warnings.is_empty() {
            println!("\n⚠️  Warnings ({}):", warnings.len());
            for warning in &warnings {
                println!("  - {warning}");
            }
        }

        if !validation_errors.is_empty() {
            anyhow::bail!(
                "Configuration validation failed with {} errors",
                validation_errors.len()
            );
        }
    }

    Ok(())
}
