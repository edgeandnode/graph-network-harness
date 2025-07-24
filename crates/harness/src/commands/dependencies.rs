//! Dependency resolution utilities for service management

use anyhow::Result;
use harness_config::Config;
use std::collections::{HashMap, HashSet};

/// Build a dependency graph from the configuration
pub fn build_dependency_graph(config: &Config) -> HashMap<String, HashSet<String>> {
    let mut graph = HashMap::new();

    // Initialize all services in the graph
    for service_name in config.services.keys() {
        graph.insert(service_name.clone(), HashSet::new());
    }

    // Build the dependency relationships
    for (service_name, service) in &config.services {
        for dep in &service.dependencies {
            if let Some(dependents) = graph.get_mut(dep) {
                dependents.insert(service_name.clone());
            }
        }
    }

    graph
}

/// Get services in reverse dependency order (dependents first)
/// This is used for stopping services safely
pub fn reverse_topological_sort(
    config: &Config,
    services: &[String],
) -> Result<Vec<String>> {
    let graph = build_dependency_graph(config);
    let mut result = Vec::new();
    let mut visited = HashSet::new();
    let mut visiting = HashSet::new();

    // If specific services are requested, we need to include their dependents
    let services_to_stop: Vec<String> = if services.is_empty() {
        config.services.keys().cloned().collect()
    } else {
        let mut to_stop = HashSet::new();
        for service in services {
            to_stop.insert(service.clone());
            collect_dependents(&graph, service, &mut to_stop);
        }
        to_stop.into_iter().collect()
    };

    // Perform DFS on each service
    for service in &services_to_stop {
        if !visited.contains(service) {
            dfs_reverse(
                service,
                &graph,
                &mut visited,
                &mut visiting,
                &mut result,
            )?;
        }
    }

    Ok(result)
}

/// Get services in forward dependency order (dependencies first)
/// This is used for starting services in correct order
pub fn topological_sort(
    config: &Config,
    services: &[String],
) -> Result<Vec<String>> {
    let mut result = Vec::new();
    let mut visited = HashSet::new();
    let mut visiting = HashSet::new();

    // If specific services are requested, we need to include their dependencies
    let services_to_start: Vec<String> = if services.is_empty() {
        config.services.keys().cloned().collect()
    } else {
        let mut to_start = HashSet::new();
        for service in services {
            to_start.insert(service.clone());
            collect_dependencies(config, service, &mut to_start)?;
        }
        to_start.into_iter().collect()
    };

    // Perform DFS on each service
    for service in &services_to_start {
        if !visited.contains(service) {
            dfs_forward(
                service,
                config,
                &mut visited,
                &mut visiting,
                &mut result,
            )?;
        }
    }

    Ok(result)
}

/// Collect all dependents of a service (services that depend on it)
fn collect_dependents(
    graph: &HashMap<String, HashSet<String>>,
    service: &str,
    collected: &mut HashSet<String>,
) {
    if let Some(dependents) = graph.get(service) {
        for dependent in dependents {
            if !collected.contains(dependent) {
                collected.insert(dependent.clone());
                collect_dependents(graph, dependent, collected);
            }
        }
    }
}

/// Collect all dependencies of a service (services it depends on)
fn collect_dependencies(
    config: &Config,
    service: &str,
    collected: &mut HashSet<String>,
) -> Result<()> {
    if let Some(service_config) = config.services.get(service) {
        for dep in &service_config.dependencies {
            if !collected.contains(dep) {
                if !config.services.contains_key(dep) {
                    anyhow::bail!("Service '{}' depends on unknown service '{}'", service, dep);
                }
                collected.insert(dep.clone());
                collect_dependencies(config, dep, collected)?;
            }
        }
    }
    Ok(())
}

/// DFS for reverse topological sort (dependents first)
fn dfs_reverse(
    service: &str,
    graph: &HashMap<String, HashSet<String>>,
    visited: &mut HashSet<String>,
    visiting: &mut HashSet<String>,
    result: &mut Vec<String>,
) -> Result<()> {
    if visiting.contains(service) {
        anyhow::bail!("Circular dependency detected involving service '{}'", service);
    }

    visiting.insert(service.to_string());

    // Visit dependents first
    if let Some(dependents) = graph.get(service) {
        for dependent in dependents {
            if !visited.contains(dependent) {
                dfs_reverse(dependent, graph, visited, visiting, result)?;
            }
        }
    }

    visiting.remove(service);
    visited.insert(service.to_string());
    result.push(service.to_string());

    Ok(())
}

/// DFS for forward topological sort (dependencies first)
fn dfs_forward(
    service: &str,
    config: &Config,
    visited: &mut HashSet<String>,
    visiting: &mut HashSet<String>,
    result: &mut Vec<String>,
) -> Result<()> {
    if visiting.contains(service) {
        anyhow::bail!("Circular dependency detected involving service '{}'", service);
    }

    visiting.insert(service.to_string());

    // Visit dependencies first
    if let Some(service_config) = config.services.get(service) {
        for dep in &service_config.dependencies {
            if !visited.contains(dep) {
                dfs_forward(dep, config, visited, visiting, result)?;
            }
        }
    }

    visiting.remove(service);
    visited.insert(service.to_string());
    result.push(service.to_string());

    Ok(())
}

/// Get services that would be affected by stopping the given services
pub fn get_affected_services(
    config: &Config,
    services: &[String],
) -> Vec<String> {
    let graph = build_dependency_graph(config);
    let mut affected = HashSet::new();

    for service in services {
        collect_dependents(&graph, service, &mut affected);
    }

    // Remove the original services from affected list
    for service in services {
        affected.remove(service);
    }

    let mut result: Vec<String> = affected.into_iter().collect();
    result.sort();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_config::Service;

    fn create_test_config() -> Config {
        let mut services = HashMap::new();

        // Create a dependency chain: app -> api -> db
        services.insert(
            "db".to_string(),
            Service {
                service_type: harness_config::ServiceType::Docker {
                    image: "postgres".to_string(),
                    ports: vec![],
                    volumes: vec![],
                    command: None,
                    entrypoint: None,
                },
                network: "local".to_string(),
                env: HashMap::new(),
                dependencies: vec![],
                health_check: None,
                startup_timeout: None,
                shutdown_timeout: None,
            },
        );

        services.insert(
            "api".to_string(),
            Service {
                service_type: harness_config::ServiceType::Process {
                    binary: "api-server".to_string(),
                    args: vec![],
                    working_dir: None,
                    user: None,
                },
                network: "local".to_string(),
                env: HashMap::new(),
                dependencies: vec!["db".to_string()],
                health_check: None,
                startup_timeout: None,
                shutdown_timeout: None,
            },
        );

        services.insert(
            "app".to_string(),
            Service {
                service_type: harness_config::ServiceType::Process {
                    binary: "app".to_string(),
                    args: vec![],
                    working_dir: None,
                    user: None,
                },
                network: "local".to_string(),
                env: HashMap::new(),
                dependencies: vec!["api".to_string()],
                health_check: None,
                startup_timeout: None,
                shutdown_timeout: None,
            },
        );

        Config {
            version: "1.0".to_string(),
            name: Some("test".to_string()),
            description: None,
            settings: Default::default(),
            networks: HashMap::new(),
            services,
        }
    }

    #[test]
    fn test_reverse_topological_sort() {
        let config = create_test_config();
        
        // When stopping all services, should stop in reverse order
        let order = reverse_topological_sort(&config, &[]).unwrap();
        assert_eq!(order, vec!["app", "api", "db"]);
        
        // When stopping just db, should stop dependent services first
        let order = reverse_topological_sort(&config, &["db".to_string()]).unwrap();
        assert_eq!(order, vec!["app", "api", "db"]);
    }

    #[test]
    fn test_topological_sort() {
        let config = create_test_config();
        
        // When starting all services, should start dependencies first
        let order = topological_sort(&config, &[]).unwrap();
        assert_eq!(order, vec!["db", "api", "app"]);
        
        // When starting just app, should start dependencies first
        let order = topological_sort(&config, &["app".to_string()]).unwrap();
        assert_eq!(order, vec!["db", "api", "app"]);
    }

    #[test]
    fn test_get_affected_services() {
        let config = create_test_config();
        
        // Stopping db affects api and app
        let affected = get_affected_services(&config, &["db".to_string()]);
        assert_eq!(affected, vec!["api", "app"]);
        
        // Stopping api affects only app
        let affected = get_affected_services(&config, &["api".to_string()]);
        assert_eq!(affected, vec!["app"]);
        
        // Stopping app affects nothing
        let affected = get_affected_services(&config, &["app".to_string()]);
        assert!(affected.is_empty());
    }
}