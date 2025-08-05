//! Factory for creating services with ServiceSetup capabilities
//!
//! This module provides a factory that creates service instances that implement
//! both the Service trait and ServiceSetup trait.

use harness_core::service::ServiceSetup;
use std::sync::Arc;

use crate::services::{AnvilService, GraphNodeService, IpfsService, PostgresService};

/// Factory for creating services with setup capabilities
pub struct ServiceFactory;

impl ServiceFactory {
    /// Create a service instance by type that implements ServiceSetup
    pub fn create_setup_service(service_type: &str) -> Option<Arc<dyn ServiceSetup>> {
        match service_type {
            "graph-node" => Some(Arc::new(GraphNodeService::default())),
            "anvil" => Some(Arc::new(AnvilService::default())),
            "postgres" => Some(Arc::new(PostgresService::default())),
            "ipfs" => Some(Arc::new(IpfsService::default())),
            _ => None,
        }
    }

    /// Create a service with specific configuration
    pub fn create_configured_service(
        service_type: &str,
        config: &serde_json::Value,
    ) -> Option<Arc<dyn ServiceSetup>> {
        match service_type {
            "graph-node" => {
                let endpoint = config
                    .get("endpoint")
                    .and_then(|v| v.as_str())
                    .unwrap_or("http://localhost:8030")
                    .to_string();
                Some(Arc::new(GraphNodeService::new(endpoint)))
            }
            "anvil" => {
                let chain_id = config
                    .get("chain_id")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1337);
                let port = config.get("port").and_then(|v| v.as_u64()).unwrap_or(8545) as u16;
                Some(Arc::new(AnvilService::new(chain_id, port)))
            }
            "postgres" => {
                let db_name = config
                    .get("db_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("graph-node")
                    .to_string();
                let port = config.get("port").and_then(|v| v.as_u64()).unwrap_or(5432) as u16;
                Some(Arc::new(PostgresService::new(db_name, port)))
            }
            "ipfs" => {
                let api_port = config
                    .get("api_port")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(5001) as u16;
                let gateway_port = config
                    .get("gateway_port")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(8080) as u16;
                Some(Arc::new(IpfsService::new(api_port, gateway_port)))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[smol_potat::test]
    async fn test_factory_creates_services() {
        // Test creating each service type
        let services = vec!["graph-node", "anvil", "postgres", "ipfs"];

        for service_type in services {
            let service = ServiceFactory::create_setup_service(service_type);
            assert!(
                service.is_some(),
                "Failed to create service: {}",
                service_type
            );

            // Verify we can call ServiceSetup methods
            let service = service.unwrap();
            let result = service.is_setup_complete().await;
            assert!(result.is_ok());
        }
    }

    #[smol_potat::test]
    async fn test_factory_with_config() {
        let config = serde_json::json!({
            "endpoint": "http://custom:8030"
        });

        let service = ServiceFactory::create_configured_service("graph-node", &config);
        assert!(service.is_some());
    }
}
