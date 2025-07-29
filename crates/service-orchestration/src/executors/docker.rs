//! Docker executor for containerized service execution.

use super::{LogStream, NetworkInfo, RunningService, ServiceExecutor};
use crate::{
    config::{ServiceConfig, ServiceTarget},
    health::{HealthChecker, HealthStatus},
    Result,
};
use async_trait::async_trait;
use command_executor::{backends::LocalLauncher, target::Target, Command, Executor};
use futures::stream::{self, StreamExt};
use tracing::{info, warn};

/// Executor for Docker container services
pub struct DockerExecutor {
    executor: Executor<LocalLauncher>,
    health_checker: HealthChecker,
}

/// Container state information
#[derive(Debug)]
struct ContainerState {
    id: String,
    state: String,
    status: String,
    is_running: bool,
}

impl DockerExecutor {
    /// Create a new Docker executor
    pub fn new() -> Self {
        Self {
            executor: Executor::new("docker-executor".to_string(), LocalLauncher),
            health_checker: HealthChecker::new(),
        }
    }

    /// Detect existing container by name
    async fn detect_existing_container(&self, name: &str) -> Result<Option<ContainerState>> {
        let container_name = format!("orchestrator-{}-harness-test", name);

        // Check if container exists
        let mut ps_cmd = Command::new("docker");
        ps_cmd.args(&[
            "ps",
            "-a",
            "--filter",
            &format!("name={}", container_name),
            "--format",
            "{{.ID}}|{{.State}}|{{.Status}}",
            "--no-trunc",
        ]);

        let result = self.executor.execute(&Target::Command, ps_cmd).await?;
        if !result.success() {
            return Ok(None);
        }

        let output = result.output.trim();
        if output.is_empty() {
            return Ok(None);
        }

        // Parse the output (ID|State|Status)
        let parts: Vec<&str> = output.split('|').collect();
        if parts.len() >= 3 {
            Ok(Some(ContainerState {
                id: parts[0].to_string(),
                state: parts[1].to_string(),
                status: parts[2].to_string(),
                is_running: parts[1] == "running",
            }))
        } else {
            Ok(None)
        }
    }

    /// Adopt an existing container as a running service
    async fn adopt_container(
        &self,
        container_state: &ContainerState,
        config: ServiceConfig,
    ) -> Result<RunningService> {
        info!(
            "Adopting existing container '{}' for service '{}'",
            &container_state.id[..12],
            config.name
        );

        // Get network information
        let network_info = self.get_container_network_info(&container_state.id).await?;

        // Create running service instance
        let running_service = RunningService::new(config.name.clone(), config)
            .with_container_id(container_state.id.clone())
            .with_network_info(network_info)
            .with_metadata("executor_type".to_string(), "docker".to_string())
            .with_metadata("adopted".to_string(), "true".to_string());

        Ok(running_service)
    }

    /// Get network information for a container
    async fn get_container_network_info(&self, container_id: &str) -> Result<NetworkInfo> {
        // Get container IP address
        let mut inspect_cmd = Command::new("docker");
        inspect_cmd.args(&[
            "inspect",
            "--format",
            "{{.NetworkSettings.IPAddress}}",
            container_id,
        ]);

        let result = self.executor.execute(&Target::Command, inspect_cmd).await?;
        if !result.success() {
            return Err(crate::Error::Config(format!(
                "Failed to get container IP: {}",
                result.output
            )));
        }

        let ip = result.output.trim().to_string();

        // Get exposed ports
        let mut port_cmd = Command::new("docker");
        port_cmd.args(&["port", container_id]);

        let port_result = self.executor.execute(&Target::Command, port_cmd).await?;
        let mut ports = Vec::new();

        if port_result.success() {
            // Parse port output (format: "80/tcp -> 0.0.0.0:8080")
            for line in port_result.output.lines() {
                if let Some((container_port, _)) = line.split_once(" -> ") {
                    if let Some((port_str, _)) = container_port.split_once('/') {
                        if let Ok(port) = port_str.parse::<u16>() {
                            ports.push(port);
                        }
                    }
                }
            }
        }

        Ok(NetworkInfo {
            ip,
            port: ports.first().copied(),
            ports,
            hostname: container_id[..12].to_string(), // Use first 12 chars as hostname
        })
    }
}

impl Default for DockerExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceExecutor for DockerExecutor {
    async fn start(&self, config: ServiceConfig) -> Result<RunningService> {
        let ServiceTarget::Docker {
            image,
            env,
            ports,
            volumes,
        } = &config.target
        else {
            return Err(crate::Error::Config(
                "DockerExecutor can only handle Docker targets".to_string(),
            ));
        };

        info!("Starting Docker service: {}", config.name);

        // Check if container already exists
        if let Some(existing) = self.detect_existing_container(&config.name).await? {
            match existing.is_running {
                true => {
                    // Container is running - adopt it
                    info!(
                        "Container 'orchestrator-{}' is already running (status: {}). Adopting it.",
                        config.name, existing.status
                    );
                    return self.adopt_container(&existing, config).await;
                }
                false => {
                    // Container exists but is not running - remove it
                    info!(
                        "Container 'orchestrator-{}' exists but is {} (status: {}). Removing it.",
                        config.name, existing.state, existing.status
                    );

                    let mut rm_cmd = Command::new("docker");
                    rm_cmd.args(&["rm", "-f", &existing.id]);
                    let result = self.executor.execute(&Target::Command, rm_cmd).await?;

                    if !result.success() {
                        warn!("Failed to remove container: {}", result.output);
                    }
                }
            }
        }

        // Build docker run command
        let mut args = vec![
            "run".to_string(),
            "-d".to_string(), // Detached mode
            "--name".to_string(),
            format!("orchestrator-{}", config.name),
        ];

        // Add environment variables
        for (key, value) in env {
            args.extend(vec!["-e".to_string(), format!("{}={}", key, value)]);
        }

        // Add port mappings
        for port in ports {
            args.extend(vec!["-p".to_string(), format!("{}:{}", port, port)]);
        }

        // Add volume mounts
        for volume in volumes {
            args.extend(vec!["-v".to_string(), volume.clone()]);
        }

        // Add image
        args.push(image.clone());

        let mut cmd = Command::new("docker");
        cmd.args(&args);
        let result = self.executor.execute(&Target::Command, cmd).await?;

        if !result.success() {
            return Err(crate::Error::Config(format!(
                "Docker run failed: {}",
                result.output.trim()
            )));
        }

        let container_id = result.output.trim().to_string();
        info!(
            "Started Docker service '{}' with container ID: {}",
            config.name, container_id
        );

        // Get network information
        let network_info = self.get_container_network_info(&container_id).await?;
        info!(
            "Container '{}' network info: IP={}, ports={:?}",
            config.name, network_info.ip, network_info.ports
        );

        // Create running service instance
        let running_service = RunningService::new(config.name.clone(), config)
            .with_container_id(container_id)
            .with_network_info(network_info)
            .with_metadata("executor_type".to_string(), "docker".to_string());

        Ok(running_service)
    }

    async fn stop(&self, service: &RunningService) -> Result<()> {
        info!("Stopping Docker service: {}", service.name);

        if let Some(container_id) = &service.container_id {
            // First check if container exists
            let mut inspect_cmd = Command::new("docker");
            inspect_cmd.args(&["inspect", "--format", "{{.State.Status}}", container_id]);
            let inspect_result = self.executor.execute(&Target::Command, inspect_cmd).await?;

            if !inspect_result.success() {
                info!(
                    "Container {} not found, nothing to stop",
                    &container_id[..12]
                );
                return Ok(());
            }

            let status = inspect_result.output.trim();
            if status == "exited" || status == "dead" {
                info!(
                    "Container {} is already stopped (status: {})",
                    &container_id[..12],
                    status
                );
                // Just remove it
                let mut rm_cmd = Command::new("docker");
                rm_cmd.args(&["rm", "-f", container_id]);
                self.executor.execute(&Target::Command, rm_cmd).await?;
                return Ok(());
            }

            // Container is running, stop it
            let mut stop_cmd = Command::new("docker");
            stop_cmd.args(&["stop", container_id]);
            let result = self.executor.execute(&Target::Command, stop_cmd).await?;

            if result.success() {
                // Remove the container after stopping
                let mut rm_cmd = Command::new("docker");
                rm_cmd.args(&["rm", container_id]);
                self.executor.execute(&Target::Command, rm_cmd).await?;

                info!(
                    "Successfully stopped and removed Docker service: {}",
                    service.name
                );
            } else {
                warn!("Failed to stop Docker service: {}", service.name);
            }
        }

        Ok(())
    }

    async fn health_check(&self, service: &RunningService) -> Result<HealthStatus> {
        if let Some(container_id) = &service.container_id {
            // Check container status
            let mut inspect_cmd = Command::new("docker");
            inspect_cmd.args(&["inspect", "--format", "{{.State.Running}}", container_id]);

            let result = self.executor.execute(&Target::Command, inspect_cmd).await?;

            if !result.success() {
                return Ok(HealthStatus::Unhealthy("Container not found".to_string()));
            }

            let running = result.output.trim();
            if running != "true" {
                return Ok(HealthStatus::Unhealthy("Container not running".to_string()));
            }
        }

        // If service has a health check configured, run it in the container
        if let Some(health_check) = &service.config.health_check {
            if let Some(container_id) = &service.container_id {
                // Run health check inside the container
                let mut exec_args = vec!["exec".to_string(), container_id.clone()];
                exec_args.push(health_check.command.clone());
                exec_args.extend(health_check.args.clone());

                let mut exec_cmd = Command::new("docker");
                exec_cmd.args(&exec_args);
                let result = self.executor.execute(&Target::Command, exec_cmd).await?;

                if result.success() {
                    Ok(HealthStatus::Healthy)
                } else {
                    Ok(HealthStatus::Unhealthy("Health check failed".to_string()))
                }
            } else {
                Ok(HealthStatus::Unhealthy("No container ID".to_string()))
            }
        } else {
            // No health check configured, assume healthy if container is running
            Ok(HealthStatus::Healthy)
        }
    }

    async fn get_logs(&self, service: &RunningService) -> Result<LogStream> {
        if let Some(container_id) = &service.container_id {
            // TODO: Implement proper log streaming with docker logs -f
            // For now, return empty stream
            let stream = stream::empty().boxed();
            Ok(stream)
        } else {
            let stream = stream::empty().boxed();
            Ok(stream)
        }
    }

    fn can_handle(&self, config: &ServiceConfig) -> bool {
        matches!(config.target, ServiceTarget::Docker { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_can_handle() {
        let executor = DockerExecutor::new();

        let docker_config = ServiceConfig {
            name: "test".to_string(),
            target: ServiceTarget::Docker {
                image: "nginx".to_string(),
                env: HashMap::new(),
                ports: vec![8080],
                volumes: vec![],
            },
            dependencies: vec![],
            health_check: None,
        };

        assert!(executor.can_handle(&docker_config));

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
