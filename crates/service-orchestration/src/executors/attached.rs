//! Generic attached service executor using layered execution.
//!
//! This module provides a single executor that can observe any existing service
//! by using the appropriate layers and commands.

use super::RunningService;
use super::traits::{AttachedService, EventStreamable};
use crate::{Error, config::ServiceConfig, executors::layered::LayerConfig};
use async_channel::Receiver;
use async_runtime_compat::Spawner;
use async_trait::async_trait;
use command_executor::event::ProcessEvent;
use command_executor::{
    Command, ProcessHandle,
    backends::LocalLauncher,
    layered::{DockerLayer, LayeredExecutor as CmdLayeredExecutor, LocalLayer, SshLayer},
    target::Target,
};
use futures::lock::Mutex;
use std::collections::HashMap;
use std::result::Result;
use std::sync::Arc;
use tracing::{debug, info};

/// Information about an attached service
struct AttachedServiceInfo {
    /// The process handle for the observation command (if any)
    observation_handle: Option<Box<dyn ProcessHandle>>,
    /// Service configuration
    config: ServiceConfig,
    /// Additional metadata
    metadata: HashMap<String, String>,
}

/// A generic executor that attaches to existing services using layered execution.
///
/// This executor can attach to:
/// - Systemd services (local or remote via SSH)
/// - Docker containers (local or remote)
/// - Local processes by PID or name
/// - Services in Docker containers on remote hosts
/// - Any combination through layer composition
pub struct LayeredAttachedExecutor {
    attached_services: Arc<Mutex<HashMap<String, AttachedServiceInfo>>>,
}

impl Default for LayeredAttachedExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl LayeredAttachedExecutor {
    /// Create a new layered attached executor
    pub fn new() -> Self {
        Self {
            attached_services: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Build a layered executor from the service configuration
    fn build_executor(
        &self,
        config: &ServiceConfig,
    ) -> Result<CmdLayeredExecutor<LocalLauncher>, Error> {
        let mut executor = CmdLayeredExecutor::new(LocalLauncher);

        // Check if the target has layers (only for Layered targets)
        if let crate::config::ServiceTarget::Layered { layers, .. } = &config.target {
            for layer in layers {
                executor = match layer {
                    LayerConfig::Local { env, working_dir } => {
                        let mut local_layer = LocalLayer::new();
                        for (k, v) in env {
                            local_layer = local_layer.with_env(k.clone(), v.clone());
                        }
                        if let Some(dir) = working_dir {
                            local_layer = local_layer.with_working_dir(dir.clone());
                        }
                        executor.with_layer(local_layer)
                    }
                    LayerConfig::Ssh {
                        host,
                        user,
                        env,
                        port,
                        identity_file,
                        options,
                    } => {
                        let mut ssh_layer = SshLayer::new(format!("{}@{}", user, host));
                        if let Some(p) = port {
                            ssh_layer = ssh_layer.with_port(*p);
                        }
                        if let Some(id_file) = identity_file {
                            ssh_layer = ssh_layer.with_identity_file(id_file.clone());
                        }
                        for opt in options {
                            ssh_layer = ssh_layer.with_option(opt.clone());
                        }
                        for (k, v) in env {
                            ssh_layer = ssh_layer.with_env(k.clone(), v.clone());
                        }
                        executor.with_layer(ssh_layer)
                    }
                    LayerConfig::Docker {
                        container,
                        user,
                        working_dir,
                        env,
                        interactive,
                        tty,
                    } => {
                        let mut docker_layer = DockerLayer::new(container.clone())
                            .with_interactive(*interactive)
                            .with_tty(*tty);
                        if let Some(u) = user {
                            docker_layer = docker_layer.with_user(u.clone());
                        }
                        if let Some(dir) = working_dir {
                            docker_layer = docker_layer.with_working_dir(dir.clone());
                        }
                        for (k, v) in env {
                            docker_layer = docker_layer.with_env(k.clone(), v.clone());
                        }
                        executor.with_layer(docker_layer)
                    }
                }
            }
        }

        Ok(executor)
    }

    /// Get the appropriate status command based on the service type
    fn get_status_command(&self, config: &ServiceConfig) -> Option<Command> {
        let env = config.target.env();

        if let Some(service_name) = env.get("SYSTEMD_SERVICE") {
            // Systemd service
            Some(
                Command::new("systemctl")
                    .arg("is-active")
                    .arg(service_name)
                    .clone(),
            )
        } else if let Some(container_name) = env.get("CONTAINER_NAME") {
            // Docker container
            Some(
                Command::new("docker")
                    .arg("inspect")
                    .arg("-f")
                    .arg("{{.State.Running}}")
                    .arg(container_name)
                    .clone(),
            )
        } else if let Some(pid) = env.get("PID") {
            // Process by PID
            Some(Command::new("kill").arg("-0").arg(pid).clone())
        } else if let Some(process_name) = env.get("PROCESS_NAME") {
            // Process by name
            Some(Command::new("pgrep").arg("-f").arg(process_name).clone())
        } else {
            None
        }
    }

    /// Get the appropriate log streaming command based on the service type
    fn get_log_command(&self, config: &ServiceConfig, follow: bool) -> Option<Command> {
        let env = config.target.env();

        if let Some(service_name) = env.get("SYSTEMD_SERVICE") {
            // Systemd service logs via journalctl
            let mut cmd = Command::new("journalctl")
                .arg("-u")
                .arg(service_name)
                .clone();
            if follow {
                cmd = cmd.arg("-f").arg("-n").arg("0").clone();
            }
            Some(cmd)
        } else if let Some(container_name) = env.get("CONTAINER_NAME") {
            // Docker container logs
            let mut cmd = Command::new("docker").arg("logs").clone();
            if follow {
                cmd = cmd.arg("-f").arg("--tail").arg("0").clone();
            }
            cmd = cmd.arg(container_name).clone();
            Some(cmd)
        } else if let Some(pid) = env.get("PID") {
            // Process logs by PID via journalctl
            let mut cmd = Command::new("journalctl")
                .arg(format!("_PID={}", pid))
                .clone();
            if follow {
                cmd = cmd.arg("-f").arg("-n").arg("0").clone();
            }
            Some(cmd)
        } else if let Some(log_file) = env.get("LOG_FILE") {
            // Custom log file
            let cmd = if follow {
                Command::new("tail").arg("-f").arg(log_file).clone()
            } else {
                Command::new("cat").arg(log_file).clone()
            };
            Some(cmd)
        } else {
            None
        }
    }

    /// Check if the service is accessible/running
    async fn check_service_status(&self, config: &ServiceConfig) -> Result<bool, Error> {
        if let Some(status_cmd) = self.get_status_command(config) {
            let executor = self.build_executor(config)?;
            let (event_stream, mut handle) = executor.execute(status_cmd, &Target::Command).await?;

            // Wait for the command to complete and check exit status
            drop(event_stream); // We don't need the events
            let exit_result = handle.wait().await?;
            Ok(exit_result.success())
        } else {
            // If no status command, assume it's accessible
            Ok(true)
        }
    }
}

#[async_trait]
impl EventStreamable for LayeredAttachedExecutor {
    async fn stream_events(
        &self,
        service: &RunningService,
        spawner: &dyn Spawner,
    ) -> Result<Receiver<ProcessEvent>, Error> {
        let attached = self.attached_services.lock().await;
        if let Some(info) = attached.get(&service.name) {
            if let Some(log_cmd) = self.get_log_command(&info.config, true) {
                let executor = self.build_executor(&info.config)?;
                let (event_stream, _handle) = executor.execute(log_cmd, &Target::Command).await?;

                // Convert stream to receiver
                let (tx, rx) = async_channel::unbounded();

                // Spawn task to forward events
                let forward_task = async move {
                    use futures::StreamExt;
                    let mut stream = event_stream;
                    while let Some(event) = stream.next().await {
                        if tx.send(event).await.is_err() {
                            break; // Receiver dropped
                        }
                    }
                };

                spawner.spawn(Box::pin(forward_task));

                Ok(rx)
            } else {
                Err(Error::NotImplemented(
                    "No log command available for this service type".to_string(),
                ))
            }
        } else {
            Err(Error::NotImplemented("Service not attached".to_string()))
        }
    }
}

#[async_trait]
impl AttachedService for LayeredAttachedExecutor {
    async fn attach(
        &self,
        config: ServiceConfig,
        _spawner: &dyn Spawner,
    ) -> Result<RunningService, Error> {
        let service_name = config.name.clone();
        info!("Attaching to service: {}", service_name);

        // Check if the service is accessible
        if !self.check_service_status(&config).await? {
            return Err(Error::Config(format!(
                "Service '{}' is not accessible or not running",
                service_name
            )));
        }

        // Optionally start log streaming (could be done on-demand instead)
        let observation_handle = if let Some(log_cmd) = self.get_log_command(&config, true) {
            debug!("Starting log streaming for {}", service_name);
            let executor = self.build_executor(&config)?;
            let (_, handle) = executor.execute(log_cmd, &Target::Command).await?;
            Some(Box::new(handle) as Box<dyn ProcessHandle>)
        } else {
            None
        };

        // Extract metadata based on service type
        let mut metadata = HashMap::new();
        let env = config.target.env();

        if let Some(systemd_service) = env.get("SYSTEMD_SERVICE") {
            metadata.insert("systemd_unit".to_string(), systemd_service.clone());
            metadata.insert("service_type".to_string(), "systemd".to_string());
        } else if let Some(container) = env.get("CONTAINER_NAME") {
            metadata.insert("container_name".to_string(), container.clone());
            metadata.insert("service_type".to_string(), "docker".to_string());
        } else if let Some(pid) = env.get("PID") {
            metadata.insert("pid".to_string(), pid.clone());
            metadata.insert("service_type".to_string(), "process".to_string());
        } else if let Some(process) = env.get("PROCESS_NAME") {
            metadata.insert("process_name".to_string(), process.clone());
            metadata.insert("service_type".to_string(), "process".to_string());
        }

        metadata.insert("attached".to_string(), "true".to_string());

        // Store the attachment info
        let info = AttachedServiceInfo {
            observation_handle,
            config: config.clone(),
            metadata: metadata.clone(),
        };

        self.attached_services
            .lock()
            .await
            .insert(service_name.clone(), info);

        // Create running service with metadata
        let mut service = RunningService::new(service_name.clone(), config);
        for (key, value) in metadata {
            service = service.with_metadata(key, value);
        }

        // Set container_id if it's a Docker service
        if let Some(container) = env.get("CONTAINER_NAME") {
            service.container_id = Some(container.clone());
        }

        // Set PID if available
        if let Some(pid_str) = env.get("PID") {
            if let Ok(pid) = pid_str.parse::<u32>() {
                service.pid = Some(pid);
            }
        }

        Ok(service)
    }

    async fn detach(&self, service: &RunningService, _spawner: &dyn Spawner) -> Result<(), Error> {
        info!("Detaching from service: {}", service.name);

        let mut attached = self.attached_services.lock().await;
        if let Some(mut info) = attached.remove(&service.name) {
            // Stop any observation process if it exists
            if let Some(mut handle) = info.observation_handle {
                debug!("Terminating observation process for {}", service.name);
                handle.terminate().await.ok();
            }
        }

        Ok(())
    }

    async fn is_accessible(
        &self,
        service: &RunningService,
        _spawner: &dyn Spawner,
    ) -> Result<bool, Error> {
        let attached = self.attached_services.lock().await;
        if let Some(info) = attached.get(&service.name) {
            self.check_service_status(&info.config).await
        } else {
            Ok(false)
        }
    }

    fn can_handle(&self, config: &ServiceConfig) -> bool {
        // This executor can handle any service that has identifiable metadata
        let env = config.target.env();
        env.contains_key("SYSTEMD_SERVICE")
            || env.contains_key("CONTAINER_NAME")
            || env.contains_key("PID")
            || env.contains_key("PROCESS_NAME")
            || env.contains_key("LOG_FILE")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServiceTarget;

    #[test]
    fn test_can_handle_systemd() {
        let executor = LayeredAttachedExecutor::new();
        let config = ServiceConfig {
            name: "test".to_string(),
            target: ServiceTarget::Process {
                command: crate::config::ProcessCommand::Legacy {
                    command: "dummy".to_string(),
                },
                env: HashMap::from([("SYSTEMD_SERVICE".to_string(), "nginx".to_string())]),
                working_dir: None,
            },
            dependencies: vec![],
            health_check: None,
        };
        assert!(executor.can_handle(&config));
    }

    #[test]
    fn test_can_handle_docker() {
        let executor = LayeredAttachedExecutor::new();
        let config = ServiceConfig {
            name: "test".to_string(),
            target: ServiceTarget::Docker {
                params: HashMap::new(),
                image: "nginx".to_string(),
                command_template: None,
                env: HashMap::from([("CONTAINER_NAME".to_string(), "my-nginx".to_string())]),
                ports: vec![],
                volumes: vec![],
            },
            dependencies: vec![],
            health_check: None,
        };
        assert!(executor.can_handle(&config));
    }

    #[test]
    fn test_can_handle_process() {
        let executor = LayeredAttachedExecutor::new();
        let config = ServiceConfig {
            name: "test".to_string(),
            target: ServiceTarget::Process {
                command: crate::config::ProcessCommand::Legacy {
                    command: "dummy".to_string(),
                },
                env: HashMap::from([("PID".to_string(), "1234".to_string())]),
                working_dir: None,
            },
            dependencies: vec![],
            health_check: None,
        };
        assert!(executor.can_handle(&config));
    }
}
