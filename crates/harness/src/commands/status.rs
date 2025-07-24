use anyhow::{Context, Result};
use comfy_table::{Table, Cell, Color};
use harness_config::parser;
use service_orchestration::ServiceStatus;
use std::path::Path;
use crate::commands::client;
use harness::protocol::{Request, Response};

pub async fn run(config_path: &Path) -> Result<()> {
    // Parse configuration to get service list
    let config = parser::parse_file(config_path)
        .context("Failed to parse configuration")?;
    
    // Connect to daemon
    let mut daemon = client::connect_to_daemon().await?;
    
    // Create table
    let mut table = Table::new();
    table.set_header(vec!["SERVICE", "STATUS", "HEALTH"]);
    
    // Get status for all services from daemon
    let request = Request::ListServices;
    let services_status = match daemon.send_request(request).await? {
        Response::ServiceList { services } => services,
        Response::Error { message } => {
            anyhow::bail!("Failed to get service list: {}", message);
        }
        _ => anyhow::bail!("Unexpected response from daemon"),
    };
    
    // Get status for each service
    for (service_name, _service) in &config.services {
        let status = services_status.get(service_name)
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