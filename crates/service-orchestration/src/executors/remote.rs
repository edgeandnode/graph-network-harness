//! Remote executor for SSH-based service execution.

use super::{LogStream, RunningService, ServiceExecutor};
use crate::{
    config::{ServiceConfig, ServiceTarget},
    health::{HealthChecker, HealthStatus},
    Result,
};
use async_trait::async_trait;
use futures::stream::{self, StreamExt};
use tracing::{info, warn};

/// Executor for remote SSH services
pub struct RemoteExecutor {
    health_checker: HealthChecker,
}

impl RemoteExecutor {
    /// Create a new remote executor
    pub fn new() -> Self {
        Self {
            health_checker: HealthChecker::new(),
        }
    }
}

impl Default for RemoteExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceExecutor for RemoteExecutor {
    async fn start(&self, config: ServiceConfig) -> Result<RunningService> {
        match config.target.clone() {
            ServiceTarget::RemoteLan {
                host,
                user,
                binary,
                args,
            } => {
                info!(
                    "Starting remote LAN service: {} on {}@{}",
                    config.name, user, host
                );

                // TODO: Implement SSH connection and command execution
                // This would use command-executor's SshLauncher

                // For now, return a placeholder
                let running_service = RunningService::new(config.name.clone(), config)
                    .with_metadata("executor_type".to_string(), "remote_lan".to_string())
                    .with_metadata("host".to_string(), host.clone())
                    .with_metadata("user".to_string(), user.clone());

                Ok(running_service)
            }
            ServiceTarget::Wireguard {
                host,
                user,
                package,
            } => {
                info!(
                    "Starting WireGuard service: {} on {}@{}",
                    config.name, user, host
                );

                // TODO: Implement package deployment and service start

                let running_service = RunningService::new(config.name.clone(), config)
                    .with_metadata("executor_type".to_string(), "wireguard".to_string())
                    .with_metadata("host".to_string(), host.clone())
                    .with_metadata("user".to_string(), user.clone())
                    .with_metadata("package".to_string(), package.clone());

                Ok(running_service)
            }
            _ => Err(crate::Error::Config(
                "RemoteExecutor can only handle RemoteLan and Wireguard targets".to_string(),
            )),
        }
    }

    async fn stop(&self, service: &RunningService) -> Result<()> {
        info!("Stopping remote service: {}", service.name);

        // TODO: Implement remote service stopping via SSH

        warn!(
            "Remote service stopping not yet implemented for: {}",
            service.name
        );
        Ok(())
    }

    async fn health_check(&self, service: &RunningService) -> Result<HealthStatus> {
        // TODO: Implement remote health checking

        // For now, return unknown
        Ok(HealthStatus::Unknown)
    }

    async fn get_logs(&self, service: &RunningService) -> Result<LogStream> {
        // TODO: Implement remote log streaming

        let stream = stream::empty().boxed();
        Ok(stream)
    }

    fn can_handle(&self, config: &ServiceConfig) -> bool {
        matches!(
            config.target,
            ServiceTarget::RemoteLan { .. } | ServiceTarget::Wireguard { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_can_handle() {
        let executor = RemoteExecutor::new();

        let remote_config = ServiceConfig {
            name: "test".to_string(),
            target: ServiceTarget::RemoteLan {
                host: "192.168.1.100".to_string(),
                user: "testuser".to_string(),
                binary: "echo".to_string(),
                args: vec!["hello".to_string()],
            },
            dependencies: vec![],
            health_check: None,
        };

        assert!(executor.can_handle(&remote_config));

        let wireguard_config = ServiceConfig {
            name: "test".to_string(),
            target: ServiceTarget::Wireguard {
                host: "10.0.0.100".to_string(),
                user: "testuser".to_string(),
                package: "/path/to/package.tar.gz".to_string(),
            },
            dependencies: vec![],
            health_check: None,
        };

        assert!(executor.can_handle(&wireguard_config));

        let process_config = ServiceConfig {
            name: "test".to_string(),
            target: ServiceTarget::Process {
                binary: "echo".to_string(),
                args: vec![],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![],
            health_check: None,
        };

        assert!(!executor.can_handle(&process_config));
    }
}
