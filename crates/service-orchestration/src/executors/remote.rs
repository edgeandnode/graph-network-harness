//! Remote executor for SSH-based service execution.

use super::{EventStream, RunningService, ServiceExecutor};
use crate::{
    Result,
    config::{ServiceConfig, ServiceTarget},
    health::{HealthChecker, HealthStatus},
};
use async_trait::async_trait;
use command_executor::{
    Command, Executor, ProcessHandle, 
    backends::{LocalLauncher, ssh::{SshConfig, SshLauncher}},
    target::Target,
    event::ProcessEvent
};
use futures::stream::{self, Stream, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use futures::lock::Mutex;
use tracing::{info, warn};

type LocalEventStream = Box<dyn Stream<Item = ProcessEvent> + Send + Unpin>;

/// Information about a remote process
struct RemoteProcessInfo {
    handle: Box<dyn ProcessHandle>,
    event_stream: Arc<Mutex<LocalEventStream>>,
    host: String,
    user: String,
}

/// Executor for remote SSH services
pub struct RemoteExecutor {
    health_checker: HealthChecker,
    running_processes: Arc<Mutex<HashMap<String, RemoteProcessInfo>>>,
}

impl RemoteExecutor {
    /// Create a new remote executor
    pub fn new() -> Self {
        Self {
            health_checker: HealthChecker::new(),
            running_processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Create an SSH executor for the given host and user
    fn create_ssh_executor(&self, host: &str, user: &str) -> Executor<SshLauncher<LocalLauncher>> {
        let ssh_config = SshConfig::new(host).with_user(user);
        let local_launcher = LocalLauncher;
        let ssh_launcher = SshLauncher::new(local_launcher, ssh_config);
        Executor::new(format!("ssh-{}@{}", user, host), ssh_launcher)
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

                // Create SSH executor
                let ssh_executor = self.create_ssh_executor(&host, &user);

                // Build the command to run remotely
                let mut cmd = Command::new(binary);
                cmd.args(args);

                // Get environment from config
                for (key, value) in config.target.env() {
                    cmd.env(key, value);
                }

                // Launch the process remotely
                let target = Target::ManagedProcess(command_executor::target::ManagedProcess::new());
                let (event_stream, handle) = ssh_executor.launch(&target, cmd).await?;
                
                // Get the PID from the handle
                let pid = handle.pid().unwrap_or(0);
                
                info!(
                    "Started remote service '{}' on {}@{} with PID: {}",
                    config.name, user, host, pid
                );

                // Create running service instance
                let running_service = RunningService::new(config.name.clone(), config)
                    .with_pid(pid)
                    .with_metadata("executor_type".to_string(), "remote_lan".to_string())
                    .with_metadata("host".to_string(), host.clone())
                    .with_metadata("user".to_string(), user.clone());
                
                // Store the process info
                {
                    let mut processes = self.running_processes.lock().await;
                    processes.insert(
                        running_service.id.to_string(),
                        RemoteProcessInfo {
                            handle: Box::new(handle) as Box<dyn ProcessHandle>,
                            event_stream: Arc::new(Mutex::new(Box::new(event_stream) as LocalEventStream)),
                            host: host.clone(),
                            user: user.clone(),
                        }
                    );
                }

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

        // Remove and get the process info
        let process_info = {
            let mut processes = self.running_processes.lock().await;
            processes.remove(&service.id.to_string())
        };

        if let Some(mut process_info) = process_info {
            info!(
                "Terminating remote service '{}' on {}@{}",
                service.name, process_info.user, process_info.host
            );
            
            // Use the handle to properly terminate the process
            match process_info.handle.terminate().await {
                Ok(_) => {
                    info!("Successfully terminated remote service: {}", service.name);
                }
                Err(e) => {
                    warn!("Failed to terminate remote service {}: {}, trying kill", service.name, e);
                    // Try force kill
                    if let Err(e) = process_info.handle.kill().await {
                        warn!("Failed to kill remote service {}: {}", service.name, e);
                    }
                }
            }
        } else if let Some(pid) = service.pid {
            // Fallback: use SSH to send kill command if we don't have a handle
            warn!("No process handle found for remote service {}, using SSH kill command", service.name);
            
            if let (Some(host), Some(user)) = (
                service.metadata.get("host"),
                service.metadata.get("user")
            ) {
                let ssh_executor = self.create_ssh_executor(host, user);
                let mut kill_cmd = Command::new("kill");
                kill_cmd.args(&[pid.to_string()]);

                match ssh_executor.execute(&Target::Command, kill_cmd).await {
                    Ok(result) => {
                        if result.success() {
                            info!("Successfully stopped remote service: {}", service.name);
                        } else {
                            warn!(
                                "Kill command failed for remote service: {}, trying SIGKILL",
                                service.name
                            );

                            // Try force kill
                            let mut force_kill_cmd = Command::new("kill");
                            force_kill_cmd.args(&["-9", &pid.to_string()]);
                            ssh_executor
                                .execute(&Target::Command, force_kill_cmd)
                                .await?;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to stop remote service {}: {}", service.name, e);
                        return Err(e.into());
                    }
                }
            } else {
                warn!("Missing host/user metadata for remote service: {}", service.name);
            }
        } else {
            warn!("No PID found for remote service: {}", service.name);
        }

        Ok(())
    }

    async fn health_check(&self, service: &RunningService) -> Result<HealthStatus> {
        // First check if process is still running via SSH
        if let (Some(pid), Some(host), Some(user)) = (
            service.pid,
            service.metadata.get("host"),
            service.metadata.get("user")
        ) {
            let ssh_executor = self.create_ssh_executor(host, user);
            let mut check_cmd = Command::new("kill");
            check_cmd.args(&["-0", &pid.to_string()]);
            
            match ssh_executor.execute(&Target::Command, check_cmd).await {
                Ok(result) => {
                    if !result.success() {
                        return Ok(HealthStatus::Unhealthy("Remote process not running".to_string()));
                    }
                }
                Err(_) => {
                    return Ok(HealthStatus::Unhealthy(
                        "Failed to check remote process".to_string(),
                    ));
                }
            }
        }

        // If service has a health check configured, run it remotely
        if let Some(health_check) = &service.config.health_check {
            if let (Some(host), Some(user)) = (
                service.metadata.get("host"),
                service.metadata.get("user")
            ) {
                let ssh_executor = self.create_ssh_executor(host, user);
                let mut cmd = Command::new(&health_check.command);
                cmd.args(&health_check.args);
                
                match ssh_executor.execute(&Target::Command, cmd).await {
                    Ok(result) => {
                        if result.success() {
                            Ok(HealthStatus::Healthy)
                        } else {
                            Ok(HealthStatus::Unhealthy(format!(
                                "Health check failed with exit code: {:?}",
                                result.code()
                            )))
                        }
                    }
                    Err(e) => {
                        Ok(HealthStatus::Unhealthy(format!(
                            "Health check error: {}",
                            e
                        )))
                    }
                }
            } else {
                Ok(HealthStatus::Unknown)
            }
        } else {
            // No health check configured, assume healthy if process is running
            Ok(HealthStatus::Healthy)
        }
    }

    async fn stream_events(&self, service: &RunningService) -> Result<EventStream> {
        // Get the event stream for this service
        let processes = self.running_processes.lock().await;
        let process_info = processes.get(&service.id.to_string())
            .ok_or_else(|| crate::Error::ServiceNotFound(service.name.clone()))?;
        
        let event_stream = process_info.event_stream.clone();
        drop(processes); // Release the lock early
        
        // Create a stream that forwards events from the stored stream
        let log_stream = stream::unfold(event_stream, |event_stream| async {
            let next_event = {
                let mut stream = event_stream.lock().await;
                stream.next().await
            };
            match next_event {
                Some(event) => Some((event, event_stream)),
                None => None,
            }
        });
        
        Ok(log_stream.boxed())
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
