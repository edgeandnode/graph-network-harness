//! Factory for creating services with ServiceSetup capabilities
//!
//! This module provides a factory that creates service instances that implement
//! both the Service trait and ServiceSetup trait.

use crate::services::{AnvilService, GraphNodeService, IpfsService, PostgresService};
use harness_core::Error;
use std::result::Result;

/// Enum containing all supported graph protocol services
#[derive(Debug)]
pub enum GraphService {
    /// Graph Node service
    GraphNode(GraphNodeService),
    /// Anvil (local Ethereum) service  
    Anvil(AnvilService),
    /// PostgreSQL database service
    Postgres(PostgresService),
    /// IPFS service
    Ipfs(IpfsService),
}

// Need to implement Service first, then ServiceSetup
#[async_trait::async_trait]
impl harness_core::service::Service for GraphService {
    type Action = serde_json::Value; // Generic action for the enum
    type Event = serde_json::Value; // Generic event for the enum

    fn service_type() -> &'static str {
        "graph-service-enum" // Generic type for the enum
    }

    fn name(&self) -> &str {
        match self {
            GraphService::GraphNode(service) => service.name(),
            GraphService::Anvil(service) => service.name(),
            GraphService::Postgres(service) => service.name(),
            GraphService::Ipfs(service) => service.name(),
        }
    }

    fn description(&self) -> &str {
        match self {
            GraphService::GraphNode(service) => service.description(),
            GraphService::Anvil(service) => service.description(),
            GraphService::Postgres(service) => service.description(),
            GraphService::Ipfs(service) => service.description(),
        }
    }

    async fn dispatch_action(
        &self,
        _action: Self::Action,
    ) -> Result<async_channel::Receiver<Self::Event>, Error> {
        // For the enum, we'd need to route actions to the appropriate service
        // For now, return an empty receiver
        let (_tx, rx) = async_channel::unbounded();
        Ok(rx)
    }
}

#[async_trait::async_trait]
impl harness_core::service::ServiceSetup for GraphService {
    async fn is_setup_complete(&self) -> Result<bool, Error> {
        match self {
            GraphService::GraphNode(service) => service.is_setup_complete().await,
            GraphService::Anvil(service) => service.is_setup_complete().await,
            GraphService::Postgres(service) => service.is_setup_complete().await,
            GraphService::Ipfs(service) => service.is_setup_complete().await,
        }
    }

    async fn perform_setup(&self) -> Result<(), Error> {
        match self {
            GraphService::GraphNode(service) => service.perform_setup().await,
            GraphService::Anvil(service) => service.perform_setup().await,
            GraphService::Postgres(service) => service.perform_setup().await,
            GraphService::Ipfs(service) => service.perform_setup().await,
        }
    }

    async fn validate_setup(&self) -> Result<(), Error> {
        match self {
            GraphService::GraphNode(service) => service.validate_setup().await,
            GraphService::Anvil(service) => service.validate_setup().await,
            GraphService::Postgres(service) => service.validate_setup().await,
            GraphService::Ipfs(service) => service.validate_setup().await,
        }
    }
}

/// Factory for creating services with setup capabilities
pub struct ServiceFactory;

impl ServiceFactory {
    /// Create a service instance by type
    pub fn create_service(service_type: &str) -> Option<GraphService> {
        match service_type {
            "graph-node" => Some(GraphService::GraphNode(GraphNodeService::default())),
            "anvil" => Some(GraphService::Anvil(AnvilService::default())),
            "postgres" => Some(GraphService::Postgres(PostgresService::default())),
            "ipfs" => Some(GraphService::Ipfs(IpfsService::default())),
            _ => None,
        }
    }

    /// Create a service with setup capabilities
    pub fn create_setup_service(service_type: &str) -> Option<GraphService> {
        Self::create_service(service_type)
    }

    /// Create a service with specific configuration
    pub fn create_configured_service(
        service_type: &str,
        config: &serde_json::Value,
    ) -> Option<GraphService> {
        match service_type {
            "graph-node" => {
                let endpoint = config
                    .get("endpoint")
                    .and_then(|v| v.as_str())
                    .unwrap_or("http://localhost:8030")
                    .to_string();
                Some(GraphService::GraphNode(GraphNodeService::new(endpoint)))
            }
            "anvil" => {
                let chain_id = config
                    .get("chain_id")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1337);
                let port = config.get("port").and_then(|v| v.as_u64()).unwrap_or(8545) as u16;
                Some(GraphService::Anvil(AnvilService::new(chain_id, port)))
            }
            "postgres" => {
                let db_name = config
                    .get("db_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("graph-node")
                    .to_string();
                let port = config.get("port").and_then(|v| v.as_u64()).unwrap_or(5432) as u16;
                Some(GraphService::Postgres(PostgresService::new(db_name, port)))
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
                Some(GraphService::Ipfs(IpfsService::new(api_port, gateway_port)))
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
