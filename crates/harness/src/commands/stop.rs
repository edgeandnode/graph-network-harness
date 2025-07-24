use anyhow::{Context, Result};
use harness_config::parser;
use std::path::Path;
use crate::commands::client;
use harness::protocol::{Request, Response};

pub async fn run(config_path: &Path, services: Vec<String>) -> Result<()> {
    // Parse configuration to get service list
    let config = parser::parse_file(config_path)
        .context("Failed to parse configuration")?;
    
    // Connect to daemon
    let mut daemon = client::connect_to_daemon().await?;
    
    // Determine which services to stop
    let services_to_stop = if services.is_empty() {
        // Stop all services
        config.services.keys().cloned().collect::<Vec<_>>()
    } else {
        // Stop only specified services
        services
    };
    
    println!("Stopping {} services...", services_to_stop.len());
    
    // Stop services in reverse dependency order (dependents first)
    let mut ordered_services = services_to_stop.clone();
    ordered_services.reverse(); // Simple approach for MVP
    
    for service_name in ordered_services {
        if !config.services.contains_key(&service_name) {
            eprintln!("Warning: Service '{}' not found in configuration", service_name);
            continue;
        }
        
        println!("Stopping {}...", service_name);
        
        // Send stop request to daemon
        let request = Request::StopService {
            name: service_name.clone(),
        };
        
        match daemon.send_request(request).await {
            Ok(Response::Success) => println!("Stopped {}", service_name),
            Ok(Response::Error { message }) => eprintln!("Warning: Failed to stop '{}': {}", service_name, message),
            Ok(_) => eprintln!("Warning: Unexpected response from daemon for '{}'", service_name),
            Err(e) => eprintln!("Warning: Failed to stop '{}': {}", service_name, e),
        }
    }
    
    println!("Stop command completed");
    Ok(())
}