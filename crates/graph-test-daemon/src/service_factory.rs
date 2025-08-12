//! Factory for creating services from configuration
//!
//! This module provides a factory that creates service instances from
//! service configuration using the ServiceFromConfig trait.

use crate::services::{AnvilService, GraphNodeService, IpfsService, PostgresService};
use harness_core::{Error, config_traits::ServiceFromConfig, daemon::DaemonBuilder};
use service_orchestration::ServiceConfig;
use std::result::Result;
use tracing::info;

/// Factory for creating Graph Protocol services
pub struct ServiceFactory;

impl ServiceFactory {
    /// Register a service with the daemon builder
    /// 
    /// This method uses the ServiceFromConfig trait to create the appropriate
    /// service instance and registers it directly with the daemon builder.
    pub fn register_service(
        builder: &mut DaemonBuilder,
        instance_name: String,
        service_type: &str,
        config: &ServiceConfig,
    ) -> Result<(), Error> {
        info!(
            "Registering service '{}' of type '{}' using target '{:?}'",
            instance_name, service_type, config.target
        );

        match service_type {
            "graph-node" => {
                let service = GraphNodeService::from_config(config)?;
                builder.register_service(instance_name, service)?;
            }
            "anvil" => {
                let service = AnvilService::from_config(config)?;
                builder.register_service(instance_name, service)?;
            }
            "postgres" => {
                let service = PostgresService::from_config(config)?;
                builder.register_service(instance_name, service)?;
            }
            "ipfs" => {
                let service = IpfsService::from_config(config)?;
                builder.register_service(instance_name, service)?;
            }
            unknown => {
                return Err(Error::service_type(format!(
                    "Unknown service type '{}'",
                    unknown
                )));
            }
        }
        
        Ok(())
    }

    /// Get the list of supported service types
    pub fn supported_types() -> Vec<&'static str> {
        vec!["graph-node", "anvil", "postgres", "ipfs"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_core::daemon::BaseDaemon;
    use service_orchestration::{ProcessCommand, ServiceTarget};

    fn create_test_config() -> ServiceConfig {
        ServiceConfig {
            name: "test-service".to_string(),
            target: ServiceTarget::Process {
                command: ProcessCommand::Legacy {
                    command: "echo test".to_string(),
                },
                env: Default::default(),
                working_dir: None,
            },
            dependencies: vec![],
            health_check: None,
        }
    }

    #[test]
    fn test_supported_types() {
        let types = ServiceFactory::supported_types();
        assert!(types.contains(&"graph-node"));
        assert!(types.contains(&"anvil"));
        assert!(types.contains(&"postgres"));
        assert!(types.contains(&"ipfs"));
    }

    #[test]
    fn test_unknown_service_type() {
        let mut builder = BaseDaemon::builder();
        let config = create_test_config();
        let result = ServiceFactory::register_service(
            &mut builder,
            "test-service".to_string(),
            "unknown-service",
            &config,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown service type"));
    }
}