use crate::commands::{client, dependencies};
use anyhow::{Context, Result};
use harness::protocol::{Request, Response};
use harness_config::parser;
use std::io::{self, Write};
use std::path::Path;

pub async fn run(
    config_path: &Path,
    services: Vec<String>,
    force: bool,
    timeout: Option<u64>,
) -> Result<()> {
    // Parse configuration to get service list
    let config = parser::parse_file(config_path).context("Failed to parse configuration")?;

    // Connect to daemon
    let mut daemon = client::connect_to_daemon().await?;

    // Get current status of all services
    let service_status = match daemon.send_request(Request::ListServices).await? {
        Response::ServiceList { services } => services,
        Response::Error { message } => {
            anyhow::bail!("Failed to get service list: {}", message);
        }
        _ => anyhow::bail!("Unexpected response from daemon"),
    };

    // Filter to only running services
    let running_services: Vec<String> = service_status
        .iter()
        .filter_map(|(name, status)| {
            if matches!(
                status,
                service_orchestration::ServiceStatus::Running
                    | service_orchestration::ServiceStatus::Starting
                    | service_orchestration::ServiceStatus::Unhealthy
            ) {
                Some(name.clone())
            } else {
                None
            }
        })
        .collect();

    // Determine which services to stop
    let services_to_stop = if services.is_empty() {
        // Stop all running services
        running_services.clone()
    } else {
        // Stop only specified services that are running
        services
            .into_iter()
            .filter(|s| running_services.contains(s))
            .collect()
    };

    if services_to_stop.is_empty() {
        println!("No services to stop");
        return Ok(());
    }

    // Check for affected services
    let affected = dependencies::get_affected_services(&config, &services_to_stop);
    if !affected.is_empty() && !force {
        eprintln!("\n⚠️  WARNING: Stopping these services will affect:");
        for service in &affected {
            eprintln!("  - {}", service);
        }
        eprint!("\nDo you want to continue? [y/N] ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted");
            return Ok(());
        }
    }

    // Get services in reverse dependency order
    let ordered_services = dependencies::reverse_topological_sort(&config, &services_to_stop)?;

    // Filter to only services that are actually running
    let ordered_services: Vec<String> = ordered_services
        .into_iter()
        .filter(|s| running_services.contains(s))
        .collect();

    println!("Stopping {} services...", ordered_services.len());

    // Track failures
    let mut failures = Vec::new();

    for service_name in &ordered_services {
        if !config.services.contains_key(service_name) {
            eprintln!(
                "Warning: Service '{}' not found in configuration",
                service_name
            );
            continue;
        }

        print!("Stopping {}...", service_name);
        io::stdout().flush()?;

        // Send stop request to daemon
        let request = Request::StopService {
            name: service_name.clone(),
        };

        match daemon.send_request(request).await {
            Ok(Response::Success) => {
                println!(" ✓");

                // Wait for service to stop if timeout specified
                if let Some(timeout_secs) = timeout {
                    let start = std::time::Instant::now();
                    loop {
                        if start.elapsed().as_secs() > timeout_secs {
                            println!("  ⚠️  Timeout waiting for service to stop");
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
                                if matches!(status, service_orchestration::ServiceStatus::Stopped) {
                                    break;
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

                if !force {
                    eprintln!("  Aborting due to failure (use --force to continue)");
                    break;
                }
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

                if !force {
                    eprintln!("  Aborting due to failure (use --force to continue)");
                    break;
                }
            }
        }
    }

    // Print summary
    let stopped_count = ordered_services.len() - failures.len();
    println!("\n{} services stopped successfully", stopped_count);

    if !failures.is_empty() {
        eprintln!("\n{} services failed to stop:", failures.len());
        for (service, error) in &failures {
            eprintln!("  - {}: {}", service, error);
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("{} services failed to stop", failures.len())
    }
}
