use crate::commands::{client, dependencies};
use anyhow::{Context, Result};
use harness::protocol::{Request, Response};
use harness_config::{parser, resolver::ResolutionContext};
use std::io::{self, Write};
use std::path::Path;

pub async fn run(config_path: &Path, services: Vec<String>) -> Result<()> {
    // Parse configuration
    let config = parser::parse_file(config_path).context("Failed to parse configuration")?;

    // Connect to daemon
    let mut daemon = client::connect_to_daemon().await?;

    // Get services in dependency order (dependencies first)
    let ordered_services = dependencies::topological_sort(&config, &services)?;

    println!("Starting {} services...", ordered_services.len());

    // Track failures
    let mut failures = Vec::new();
    let mut started_services = Vec::new();

    // Create resolution context for environment variables and service references
    let mut resolution_context = ResolutionContext::new();

    for service_name in &ordered_services {
        let service_def = config.services.get(service_name).ok_or_else(|| {
            anyhow::anyhow!("Service '{}' not found in configuration", service_name)
        })?;

        // Show progress
        print!("Starting {}...", service_name);
        io::stdout().flush()?;

        // Convert from harness_config types to service_orchestration types with resolution context
        let service_config = match parser::convert_to_orchestrator_with_context(
            &config,
            service_name,
            Some(&resolution_context),
        ) {
            Ok(config) => config,
            Err(e) => {
                println!(" ✗");
                eprintln!("  Error: Failed to convert service config: {}", e);
                failures.push((service_name.clone(), e.to_string()));
                continue;
            }
        };

        // Send start request to daemon
        let request = Request::StartService {
            name: service_name.clone(),
            config: service_config,
        };

        match daemon.send_request(request).await {
            Ok(Response::ServiceStarted {
                name: _,
                network_info,
            }) => {
                println!(" ✓");
                started_services.push(service_name.clone());

                // Update resolution context with actual service network info from daemon
                resolution_context.add_service(
                    service_name.clone(),
                    network_info.ip,
                    network_info.port,
                    network_info.hostname,
                );

                // Wait for service to be running if it has a health check
                if service_def.health_check.is_some() {
                    print!("  Waiting for health check...");
                    io::stdout().flush()?;

                    let start = std::time::Instant::now();
                    let timeout =
                        std::time::Duration::from_secs(service_def.startup_timeout.unwrap_or(60));

                    loop {
                        if start.elapsed() > timeout {
                            println!(" ⚠️  Timeout");
                            break;
                        }

                        // Check service status
                        match daemon
                            .send_request(Request::GetServiceStatus {
                                name: service_name.clone(),
                            })
                            .await
                        {
                            Ok(Response::ServiceStatus { status }) => {
                                match status {
                                    service_orchestration::ServiceStatus::Running => {
                                        println!(" ✓");
                                        break;
                                    }
                                    service_orchestration::ServiceStatus::Failed(msg) => {
                                        println!(" ✗");
                                        eprintln!("    Service failed: {}", msg);
                                        break;
                                    }
                                    service_orchestration::ServiceStatus::Unhealthy => {
                                        // Keep waiting, might recover
                                    }
                                    _ => {
                                        // Still starting
                                    }
                                }
                            }
                            _ => break,
                        }

                        smol::Timer::after(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
            Ok(Response::Success) => {
                // Fallback for old daemon behavior
                println!(" ✓");
                started_services.push(service_name.clone());

                // Use default localhost values if daemon doesn't provide network info
                resolution_context.add_service(
                    service_name.clone(),
                    "127.0.0.1".to_string(),
                    None,
                    format!("{}.local", service_name),
                );

                // Wait for service to be running if it has a health check
                if service_def.health_check.is_some() {
                    print!("  Waiting for health check...");
                    io::stdout().flush()?;

                    let start = std::time::Instant::now();
                    let timeout =
                        std::time::Duration::from_secs(service_def.startup_timeout.unwrap_or(60));

                    loop {
                        if start.elapsed() > timeout {
                            println!(" ⚠️  Timeout");
                            break;
                        }

                        // Check service status
                        match daemon
                            .send_request(Request::GetServiceStatus {
                                name: service_name.clone(),
                            })
                            .await
                        {
                            Ok(Response::ServiceStatus { status }) => {
                                match status {
                                    service_orchestration::ServiceStatus::Running => {
                                        println!(" ✓");
                                        break;
                                    }
                                    service_orchestration::ServiceStatus::Failed(msg) => {
                                        println!(" ✗");
                                        eprintln!("    Service failed: {}", msg);
                                        break;
                                    }
                                    service_orchestration::ServiceStatus::Unhealthy => {
                                        // Keep waiting, might recover
                                    }
                                    _ => {
                                        // Still starting
                                    }
                                }
                            }
                            _ => break,
                        }

                        smol::Timer::after(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
            Ok(Response::Error { message }) => {
                println!(" ✗");
                eprintln!("  Error: {}", message);
                failures.push((service_name.clone(), message));

                // Stop on failure - dependencies won't work
                eprintln!("  Aborting due to failure (dependent services cannot start)");
                break;
            }
            Ok(_) => {
                println!(" ✗");
                eprintln!("  Unexpected response from daemon");
                failures.push((service_name.clone(), "Unexpected response".to_string()));
            }
            Err(e) => {
                println!(" ✗");
                eprintln!("  Error: {}", e);
                failures.push((service_name.clone(), e.to_string()));
                break;
            }
        }
    }

    // Print summary
    println!("\n{} services started successfully", started_services.len());

    if !failures.is_empty() {
        eprintln!("\n{} services failed to start:", failures.len());
        for (service, error) in &failures {
            eprintln!("  - {}: {}", service, error);
        }

        // Show which services were not started due to failures
        let not_started: Vec<String> = ordered_services
            .into_iter()
            .filter(|s| {
                !started_services.contains(s) && !failures.iter().any(|(name, _)| name == s)
            })
            .collect();

        if !not_started.is_empty() {
            eprintln!(
                "\n{} services were not started due to dependency failures:",
                not_started.len()
            );
            for service in &not_started {
                eprintln!("  - {}", service);
            }
        }
    }

    // Show service endpoints if all started successfully
    if failures.is_empty() && !started_services.is_empty() {
        println!("\nService endpoints:");
        for service_name in &started_services {
            if let Some(service_def) = config.services.get(service_name) {
                match &service_def.service_type {
                    harness_config::ServiceType::Docker { ports, .. } => {
                        for port in ports {
                            match port {
                                harness_config::PortMapping::Simple(p) => {
                                    println!("  {}: http://localhost:{}", service_name, p);
                                }
                                harness_config::PortMapping::Full(mapping) => {
                                    if let Some((host, _)) = mapping.split_once(':') {
                                        println!("  {}: http://localhost:{}", service_name, host);
                                    }
                                }
                            }
                        }
                    }
                    harness_config::ServiceType::Process { .. } => {
                        // Could check health check for endpoint info
                        if let Some(health_check) = &service_def.health_check {
                            match &health_check.check_type {
                                harness_config::HealthCheckType::Http { http } => {
                                    println!("  {}: {}", service_name, http);
                                }
                                harness_config::HealthCheckType::Tcp { tcp } => {
                                    println!("  {}: tcp://localhost:{}", service_name, tcp.port);
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("{} services failed to start", failures.len())
    }
}
