use crate::commands::client;
use anyhow::{Context, Result};
use harness::protocol::{Request, Response};
use harness_config::parser;
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub async fn run(config_path: &Path, services: Vec<String>) -> Result<()> {
    // Parse configuration
    let config = parser::parse_file(config_path).context("Failed to parse configuration")?;

    // Connect to daemon
    let mut daemon = client::connect_to_daemon().await?;

    // Determine which services to start
    let services_to_start = if services.is_empty() {
        // Start all services
        config.services.keys().cloned().collect()
    } else {
        // Start specified services and their dependencies
        resolve_dependencies(&config.services, &services)
    };

    println!("Starting {} services...", services_to_start.len());

    // Start services in dependency order
    let ordered_services = topological_sort(&config.services, &services_to_start)?;

    for service_name in ordered_services {
        println!("Starting {}...", service_name);

        // Convert to orchestrator config
        let service_config = parser::convert_to_orchestrator(&config, &service_name)
            .context(format!("Failed to convert config for '{}'", service_name))?;

        // Send start request to daemon
        let request = Request::StartService {
            name: service_name.clone(),
            config: service_config,
        };

        let response = daemon
            .send_request(request)
            .await
            .context(format!("Failed to start '{}'", service_name))?;

        // Check response
        match response {
            Response::Success => {}
            Response::Error { message } => {
                anyhow::bail!("Failed to start '{}': {}", service_name, message);
            }
            _ => anyhow::bail!("Unexpected response from daemon"),
        }

        println!("Started {}", service_name);
    }

    println!("All services started successfully");
    Ok(())
}

/// Resolve all dependencies for the given services
fn resolve_dependencies(
    all_services: &HashMap<String, harness_config::Service>,
    requested: &[String],
) -> Vec<String> {
    let mut to_start = HashSet::new();
    let mut to_process: Vec<String> = requested.to_vec();

    while let Some(service) = to_process.pop() {
        if to_start.insert(service.clone()) {
            if let Some(svc) = all_services.get(&service) {
                for dep in &svc.dependencies {
                    to_process.push(dep.clone());
                }
            }
        }
    }

    to_start.into_iter().collect()
}

/// Sort services in dependency order (dependencies first)
fn topological_sort(
    all_services: &HashMap<String, harness_config::Service>,
    services_to_start: &[String],
) -> Result<Vec<String>> {
    let mut sorted = Vec::new();
    let mut visited = HashSet::new();
    let mut visiting = HashSet::new();

    for service in services_to_start {
        visit_service(
            service,
            all_services,
            &mut visited,
            &mut visiting,
            &mut sorted,
        )?;
    }

    Ok(sorted)
}

fn visit_service(
    service: &str,
    all_services: &HashMap<String, harness_config::Service>,
    visited: &mut HashSet<String>,
    visiting: &mut HashSet<String>,
    sorted: &mut Vec<String>,
) -> Result<()> {
    if visited.contains(service) {
        return Ok(());
    }

    if visiting.contains(service) {
        anyhow::bail!("Circular dependency detected involving '{}'", service);
    }

    visiting.insert(service.to_string());

    if let Some(svc) = all_services.get(service) {
        for dep in &svc.dependencies {
            visit_service(dep, all_services, visited, visiting, sorted)?;
        }
    }

    visiting.remove(service);
    visited.insert(service.to_string());
    sorted.push(service.to_string());

    Ok(())
}
