//! WebSocket request handlers for the daemon

use crate::daemon::server::DaemonState;
use crate::protocol::{Request, Response, ServiceNetworkInfo};
use anyhow::Result;
use std::sync::Arc;
use tracing::{debug, error, info};

/// Handle a request from a client
pub async fn handle_request(request: Request, state: Arc<DaemonState>) -> Result<Response> {
    debug!("Handling request: {:?}", request);

    match request {
        Request::StartService { name, config } => {
            info!("Starting service: {}", name);
            
            match state.service_manager.start_service(&name, config).await {
                Ok(running_service) => {
                    // Get network information from the running service
                    let network_info = if let Some(net_info) = &running_service.network_info {
                        ServiceNetworkInfo {
                            ip: net_info.ip.clone(),
                            port: net_info.port,
                            hostname: net_info.hostname.clone(),
                            ports: net_info.ports.clone(),
                        }
                    } else {
                        // Fallback if no network info available
                        ServiceNetworkInfo {
                            ip: "127.0.0.1".to_string(),
                            port: None,
                            hostname: format!("{}.local", name),
                            ports: Vec::new(),
                        }
                    };
                    
                    Ok(Response::ServiceStarted {
                        name: name.clone(),
                        network_info,
                    })
                }
                Err(e) => Ok(Response::Error {
                    message: format!("Failed to start service: {}", e),
                }),
            }
        }

        Request::StopService { name } => {
            info!("Stopping service: {}", name);
            match state.service_manager.stop_service(&name).await {
                Ok(_) => Ok(Response::Success),
                Err(e) => Ok(Response::Error {
                    message: format!("Failed to stop service: {}", e),
                }),
            }
        }

        Request::GetServiceStatus { name } => {
            match state.service_manager.get_service_status(&name).await {
                Ok(status) => Ok(Response::ServiceStatus { status }),
                Err(e) => Ok(Response::Error {
                    message: format!("Failed to get service status: {}", e),
                }),
            }
        }

        Request::ListServices => {
            // Get all services and their status
            let services = match state.service_manager.list_services().await {
                Ok(services) => services,
                Err(e) => {
                    return Ok(Response::Error {
                        message: format!("Failed to list services: {}", e),
                    })
                }
            };

            let mut service_status = std::collections::HashMap::new();
            for service_name in services {
                match state
                    .service_manager
                    .get_service_status(&service_name)
                    .await
                {
                    Ok(status) => {
                        service_status.insert(service_name, status);
                    }
                    Err(e) => {
                        error!("Failed to get status for service {}: {}", service_name, e);
                    }
                }
            }

            Ok(Response::ServiceList {
                services: service_status,
            })
        }

        Request::RunHealthChecks => match state.service_manager.run_health_checks().await {
            Ok(results) => {
                let results_str = results
                    .into_iter()
                    .map(|(k, v)| (k, format!("{:?}", v)))
                    .collect();
                Ok(Response::HealthCheckResults {
                    results: results_str,
                })
            }
            Err(e) => Ok(Response::Error {
                message: format!("Failed to run health checks: {}", e),
            }),
        },

        Request::Shutdown => {
            info!("Shutdown requested");
            // For now, just return success
            // In a real implementation, we'd trigger a graceful shutdown
            Ok(Response::Success)
        }
    }
}
