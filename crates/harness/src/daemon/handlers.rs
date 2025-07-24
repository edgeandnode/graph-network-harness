//! WebSocket request handlers for the daemon

use anyhow::Result;
use crate::protocol::{Request, Response};
use crate::daemon::server::DaemonState;
use std::sync::Arc;
use tracing::{info, error, debug};

/// Handle a request from a client
pub async fn handle_request(
    request: Request,
    state: Arc<DaemonState>,
) -> Result<Response> {
    debug!("Handling request: {:?}", request);
    
    match request {
        Request::StartService { name, config } => {
            info!("Starting service: {}", name);
            // Lock the service manager
            let manager = state.service_manager.lock().unwrap();
            // We need to use block_on since the manager methods are async
            match smol::block_on(manager.start_service(&name, config)) {
                Ok(_) => Ok(Response::Success),
                Err(e) => Ok(Response::Error { 
                    message: format!("Failed to start service: {}", e) 
                }),
            }
        }
        
        Request::StopService { name } => {
            info!("Stopping service: {}", name);
            let manager = state.service_manager.lock().unwrap();
            match smol::block_on(manager.stop_service(&name)) {
                Ok(_) => Ok(Response::Success),
                Err(e) => Ok(Response::Error { 
                    message: format!("Failed to stop service: {}", e) 
                }),
            }
        }
        
        Request::GetServiceStatus { name } => {
            let manager = state.service_manager.lock().unwrap();
            match smol::block_on(manager.get_service_status(&name)) {
                Ok(status) => Ok(Response::ServiceStatus { status }),
                Err(e) => Ok(Response::Error { 
                    message: format!("Failed to get service status: {}", e) 
                }),
            }
        }
        
        Request::ListServices => {
            // Get all services and their status
            let manager = state.service_manager.lock().unwrap();
            let services = match smol::block_on(manager.list_services()) {
                Ok(services) => services,
                Err(e) => return Ok(Response::Error { 
                    message: format!("Failed to list services: {}", e) 
                }),
            };
            
            let mut service_status = std::collections::HashMap::new();
            for service_name in services {
                match smol::block_on(manager.get_service_status(&service_name)) {
                    Ok(status) => {
                        service_status.insert(service_name, status);
                    }
                    Err(e) => {
                        error!("Failed to get status for service {}: {}", service_name, e);
                    }
                }
            }
            
            Ok(Response::ServiceList { services: service_status })
        }
        
        Request::RunHealthChecks => {
            let manager = state.service_manager.lock().unwrap();
            match smol::block_on(manager.run_health_checks()) {
                Ok(results) => {
                    let results_str = results.into_iter()
                        .map(|(k, v)| (k, format!("{:?}", v)))
                        .collect();
                    Ok(Response::HealthCheckResults { results: results_str })
                }
                Err(e) => Ok(Response::Error { 
                    message: format!("Failed to run health checks: {}", e) 
                }),
            }
        }
        
        Request::Shutdown => {
            info!("Shutdown requested");
            // For now, just return success
            // In a real implementation, we'd trigger a graceful shutdown
            Ok(Response::Success)
        }
    }
}