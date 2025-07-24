use crate::commands::client;
use anyhow::{Context, Result};
use comfy_table::{Cell, Color, Table};
use harness::protocol::{Request, Response, DetailedServiceInfo};
use harness_config::parser;
use service_orchestration::ServiceStatus;
use std::path::Path;
use std::time::Duration;

pub async fn run(config_path: &Path, format: String, watch: bool, detailed: bool) -> Result<()> {
    // Validate format
    if format != "table" && format != "json" {
        anyhow::bail!("Invalid format: {}. Must be 'table' or 'json'", format);
    }

    if watch {
        run_watch_mode(config_path, &format, detailed).await
    } else {
        run_once(config_path, &format, detailed).await
    }
}

async fn run_once(config_path: &Path, format: &str, detailed: bool) -> Result<()> {
    // Parse configuration to get service list
    let config = parser::parse_file(config_path).context("Failed to parse configuration")?;

    // Connect to daemon
    let mut daemon = client::connect_to_daemon().await?;

    if detailed {
        // Get detailed service information
        let request = Request::ListServicesDetailed;
        let detailed_services = match daemon.send_request(request).await? {
            Response::ServiceListDetailed { services } => services,
            Response::Error { message } => {
                anyhow::bail!("Failed to get detailed service list: {}", message);
            }
            _ => anyhow::bail!("Unexpected response from daemon"),
        };

        if format == "json" {
            println!("{}", serde_json::to_string_pretty(&detailed_services)?);
        } else {
            display_detailed_table(&detailed_services, &config)?;
        }
    } else {
        // Get basic service status
        let request = Request::ListServices;
        let services_status = match daemon.send_request(request).await? {
            Response::ServiceList { services } => services,
            Response::Error { message } => {
                anyhow::bail!("Failed to get service list: {}", message);
            }
            _ => anyhow::bail!("Unexpected response from daemon"),
        };

        if format == "json" {
            println!("{}", serde_json::to_string_pretty(&services_status)?);
        } else {
            display_basic_table(&services_status, &config)?;
        }
    }

    Ok(())
}

async fn run_watch_mode(config_path: &Path, format: &str, detailed: bool) -> Result<()> {
    println!("Watch mode - Press Ctrl+C to exit\n");
    
    loop {
        // Clear screen and move cursor to top
        print!("\x1B[2J\x1B[1;1H");
        
        if let Err(e) = run_once(config_path, format, detailed).await {
            eprintln!("Error: {}", e);
        }
        
        // Wait 2 seconds before next refresh
        smol::Timer::after(Duration::from_secs(2)).await;
    }
}

fn display_basic_table(services_status: &std::collections::HashMap<String, ServiceStatus>, config: &harness_config::Config) -> Result<()> {
    let mut table = Table::new();
    table.set_header(vec!["SERVICE", "STATUS", "HEALTH"]);

    // Get status for each service
    for (service_name, service_config) in &config.services {
        let status = services_status
            .get(service_name)
            .cloned()
            .unwrap_or(ServiceStatus::Stopped);

        let (status_str, status_color, health_str) = match status {
            ServiceStatus::Stopped => ("stopped", Color::DarkGrey, "-"),
            ServiceStatus::Starting => ("starting", Color::Yellow, "..."),
            ServiceStatus::Running => ("running", Color::Green, "healthy"),
            ServiceStatus::Unhealthy => ("running", Color::Red, "unhealthy"),
            ServiceStatus::Failed(ref msg) => ("failed", Color::Red, msg.as_str()),
        };

        table.add_row(vec![
            Cell::new(service_name),
            Cell::new(status_str).fg(status_color),
            Cell::new(health_str),
        ]);
    }

    println!("{}", table);
    Ok(())
}

fn display_detailed_table(detailed_services: &[DetailedServiceInfo], config: &harness_config::Config) -> Result<()> {
    let mut table = Table::new();
    table.set_header(vec![
        "SERVICE", "STATUS", "NETWORK", "PID/CONTAINER", "DEPENDENCIES", "ENDPOINTS"
    ]);

    // Create a map for quick lookup
    let service_map: std::collections::HashMap<_, _> = detailed_services
        .iter()
        .map(|s| (s.name.clone(), s))
        .collect();

    // Display services in config order
    for (service_name, service_config) in &config.services {
        let service_info = service_map.get(service_name);
        
        let (status_str, status_color) = if let Some(info) = service_info {
            match info.status {
                ServiceStatus::Stopped => ("stopped", Color::DarkGrey),
                ServiceStatus::Starting => ("starting", Color::Yellow),
                ServiceStatus::Running => ("running", Color::Green),
                ServiceStatus::Unhealthy => ("unhealthy", Color::Red),
                ServiceStatus::Failed(ref msg) => ("failed", Color::Red),
            }
        } else {
            ("unknown", Color::DarkGrey)
        };

        let network_info = if let Some(info) = service_info {
            if let Some(net) = &info.network_info {
                if let Some(port) = net.port {
                    format!("{}:{}", net.ip, port)
                } else {
                    net.ip.clone()
                }
            } else {
                "-".to_string()
            }
        } else {
            "-".to_string()
        };

        let process_info = if let Some(info) = service_info {
            if let Some(pid) = info.pid {
                format!("PID {}", pid)
            } else if let Some(container) = &info.container_id {
                format!("Container {}", &container[..12])
            } else {
                "-".to_string()
            }
        } else {
            "-".to_string()
        };

        let dependencies = service_config.dependencies.join(", ");
        let deps_display = if dependencies.is_empty() { "-" } else { &dependencies };

        let endpoints = if let Some(info) = service_info {
            if info.endpoints.is_empty() {
                "-".to_string()
            } else {
                info.endpoints
                    .iter()
                    .map(|(k, v)| format!("{}:{}", k, v))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        } else {
            "-".to_string()
        };

        table.add_row(vec![
            Cell::new(service_name),
            Cell::new(status_str).fg(status_color),
            Cell::new(&network_info),
            Cell::new(&process_info),
            Cell::new(deps_display),
            Cell::new(&endpoints),
        ]);
    }

    println!("{}", table);
    Ok(())
}
