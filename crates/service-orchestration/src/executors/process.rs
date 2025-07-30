//! Process executor for local service execution.

use super::{LogStream, RunningService, ServiceExecutor};
use crate::{
    Result,
    config::{ServiceConfig, ServiceTarget},
    health::{HealthChecker, HealthStatus},
};
use async_trait::async_trait;
use command_executor::{Command, Executor, ProcessHandle, backends::LocalLauncher, target::Target};
use futures::stream::{self, StreamExt};
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// Executor for local process services
pub struct ProcessExecutor {
    executor: Executor<LocalLauncher>,
    health_checker: HealthChecker,
    running_processes: HashMap<String, Box<dyn ProcessHandle>>,
}

impl ProcessExecutor {
    /// Create a new process executor
    pub fn new() -> Self {
        Self {
            executor: Executor::new("process-executor".to_string(), LocalLauncher),
            health_checker: HealthChecker::new(),
            running_processes: HashMap::new(),
        }
    }
}

impl Default for ProcessExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceExecutor for ProcessExecutor {
    async fn start(&self, config: ServiceConfig) -> Result<RunningService> {
        let ServiceTarget::Process {
            binary,
            args,
            env,
            working_dir,
        } = &config.target
        else {
            return Err(crate::Error::Config(
                "ProcessExecutor can only handle Process targets".to_string(),
            ));
        };

        info!("Starting process service: {}", config.name);
        debug!("Command: {} {}", binary, args.join(" "));

        // Build command
        let mut cmd = Command::new(binary);
        cmd.args(args);

        // Set environment variables
        for (key, value) in env {
            cmd.env(key, value);
        }

        // Set working directory if specified
        if let Some(wd) = working_dir {
            cmd.current_dir(wd);
        }

        // Execute the command using Command target (for simple one-off commands)
        let result = self.executor.execute(&Target::Command, cmd).await?;
        let pid = 0; // TODO: Get actual PID from result

        info!(
            "Started process service '{}' with PID: {}",
            config.name, pid
        );

        // Create running service instance
        let running_service = RunningService::new(config.name.clone(), config)
            .with_pid(pid)
            .with_metadata("executor_type".to_string(), "process".to_string());

        Ok(running_service)
    }

    async fn stop(&self, service: &RunningService) -> Result<()> {
        info!("Stopping service: {}", service.name);

        if let Some(pid) = service.pid {
            // For now, we'll use a simple kill command
            // In a more sophisticated implementation, we'd track the actual process handle
            let mut kill_cmd = Command::new("kill");
            kill_cmd.args(&[pid.to_string()]);

            match self.executor.execute(&Target::Command, kill_cmd).await {
                Ok(result) => {
                    if result.success() {
                        info!("Successfully stopped service: {}", service.name);
                    } else {
                        warn!(
                            "Kill command failed for service: {}, trying SIGKILL",
                            service.name
                        );

                        // Try force kill
                        let mut force_kill_cmd = Command::new("kill");
                        force_kill_cmd.args(["-9", &pid.to_string()]);
                        self.executor
                            .execute(&Target::Command, force_kill_cmd)
                            .await?;
                    }
                }
                Err(e) => {
                    warn!("Failed to stop service {}: {}", service.name, e);
                    return Err(e.into());
                }
            }
        } else {
            warn!("No PID found for service: {}", service.name);
        }

        Ok(())
    }

    async fn health_check(&self, service: &RunningService) -> Result<HealthStatus> {
        // Check if process is still running first
        if let Some(pid) = service.pid {
            let mut check_cmd = Command::new("kill");
            check_cmd.args(["-0", &pid.to_string()]);
            match self.executor.execute(&Target::Command, check_cmd).await {
                Ok(result) => {
                    if !result.success() {
                        return Ok(HealthStatus::Unhealthy("Process not running".to_string()));
                    }
                }
                Err(_) => {
                    return Ok(HealthStatus::Unhealthy(
                        "Failed to check process".to_string(),
                    ));
                }
            }
        }

        // If service has a health check configured, run it
        if let Some(health_check) = &service.config.health_check {
            self.health_checker.check_health(health_check).await
        } else {
            // No health check configured, assume healthy if process is running
            Ok(HealthStatus::Healthy)
        }
    }

    async fn get_logs(&self, service: &RunningService) -> Result<LogStream> {
        // For process services, we could potentially tail logs from a log file
        // or use journalctl if it's a systemd service
        // For now, return an empty stream
        let stream = stream::empty().boxed();
        Ok(stream)
    }

    fn can_handle(&self, config: &ServiceConfig) -> bool {
        matches!(config.target, ServiceTarget::Process { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ServiceConfig, ServiceTarget};
    use std::collections::HashMap;

    #[test]
    fn test_can_handle() {
        let executor = ProcessExecutor::new();

        let process_config = ServiceConfig {
            name: "test".to_string(),
            target: ServiceTarget::Process {
                binary: "echo".to_string(),
                args: vec!["hello".to_string()],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![],
            health_check: None,
        };

        assert!(executor.can_handle(&process_config));

        let docker_config = ServiceConfig {
            name: "test".to_string(),
            target: ServiceTarget::Docker {
                image: "nginx".to_string(),
                env: HashMap::new(),
                ports: vec![],
                volumes: vec![],
            },
            dependencies: vec![],
            health_check: None,
        };

        assert!(!executor.can_handle(&docker_config));
    }

    #[smol_potat::test]
    async fn test_start_simple_process() {
        let executor = ProcessExecutor::new();

        let config = ServiceConfig {
            name: "echo-test".to_string(),
            target: ServiceTarget::Process {
                binary: "echo".to_string(),
                args: vec!["hello world".to_string()],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![],
            health_check: None,
        };

        let service = executor.start(config).await.unwrap();
        assert_eq!(service.name, "echo-test");
        assert!(service.pid.is_some());
    }
}
