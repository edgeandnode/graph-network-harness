//! Layered executor for flexible service execution with composed layers.

use super::{RunningService, ServiceExecutor};
use crate::{
    Error,
    config::{ServiceConfig, ServiceTarget},
    health::{HealthChecker, HealthStatus},
};
use async_channel::Receiver;
use async_runtime_compat::Spawner;
use async_trait::async_trait;
use command_executor::event::ProcessEvent;
use command_executor::{
    Command, ProcessHandle,
    backends::LocalLauncher,
    layered::{DockerLayer, LayeredExecutor as CmdLayeredExecutor, LocalLayer, SshLayer},
};
use futures::lock::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Configuration for execution layers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum LayerConfig {
    /// Local execution layer
    #[serde(rename = "local")]
    Local {
        /// Environment variables for the local execution
        #[serde(default)]
        env: HashMap<String, String>,
        /// Working directory for the local execution
        working_dir: Option<String>,
    },
    /// SSH execution layer
    #[serde(rename = "ssh")]
    Ssh {
        /// SSH host to connect to
        host: String,
        /// SSH user to connect as
        user: String,
        /// Environment variables for the SSH session
        #[serde(default)]
        env: HashMap<String, String>,
        /// SSH port (defaults to 22)
        port: Option<u16>,
        /// Path to SSH identity file (private key)
        identity_file: Option<String>,
        /// Additional SSH options
        #[serde(default)]
        options: Vec<String>,
    },
    /// Docker execution layer
    #[serde(rename = "docker")]
    Docker {
        /// Docker container name or ID
        container: String,
        /// User to run commands as in the container
        user: Option<String>,
        /// Working directory inside the container
        working_dir: Option<String>,
        /// Environment variables for the container
        #[serde(default)]
        env: HashMap<String, String>,
        /// Whether to run in interactive mode
        #[serde(default)]
        interactive: bool,
        /// Whether to allocate a pseudo-TTY
        #[serde(default)]
        tty: bool,
    },
}

/// Information about a running layered process
struct LayeredProcessInfo {
    handle: Box<dyn ProcessHandle>,
    event_receiver: Receiver<ProcessEvent>,
    layers: Vec<LayerConfig>,
}

/// Executor for layered service execution
///
/// This executor allows arbitrary composition of execution layers,
/// enabling complex scenarios like SSH + Docker, multi-hop SSH, etc.
pub struct LayeredServiceExecutor {
    health_checker: HealthChecker,
    running_processes: Arc<Mutex<HashMap<String, LayeredProcessInfo>>>,
}

impl LayeredServiceExecutor {
    /// Create a new layered service executor
    pub fn new() -> Self {
        Self {
            health_checker: HealthChecker::new(),
            running_processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Build a layered executor from configuration
    fn build_executor(
        layers: &[LayerConfig],
        env: &HashMap<String, String>,
    ) -> CmdLayeredExecutor<LocalLauncher> {
        let mut executor = CmdLayeredExecutor::new(LocalLauncher);

        for layer_config in layers {
            match layer_config {
                LayerConfig::Local {
                    env: layer_env,
                    working_dir,
                } => {
                    let mut local_layer = LocalLayer::new();

                    // Apply layer-specific environment
                    for (key, value) in layer_env {
                        local_layer = local_layer.with_env(key, value);
                    }

                    // Apply global environment
                    for (key, value) in env {
                        local_layer = local_layer.with_env(key, value);
                    }

                    if let Some(wd) = working_dir {
                        local_layer = local_layer.with_working_dir(wd);
                    }

                    executor = executor.with_layer(local_layer);
                }

                LayerConfig::Ssh {
                    host,
                    user,
                    env: layer_env,
                    port,
                    identity_file,
                    options,
                } => {
                    let destination = format!("{user}@{host}");
                    let mut ssh_layer = SshLayer::new(destination);

                    // Apply layer-specific environment
                    for (key, value) in layer_env {
                        ssh_layer = ssh_layer.with_env(key, value);
                    }

                    // Apply global environment
                    for (key, value) in env {
                        ssh_layer = ssh_layer.with_env(key, value);
                    }

                    // Apply SSH-specific configuration
                    if let Some(p) = port {
                        ssh_layer = ssh_layer.with_port(*p);
                    }

                    if let Some(key_file) = identity_file {
                        ssh_layer = ssh_layer.with_identity_file(key_file);
                    }

                    for option in options {
                        ssh_layer = ssh_layer.with_option(option);
                    }

                    // Enable agent forwarding by default
                    ssh_layer = ssh_layer.with_agent_forwarding(true);

                    executor = executor.with_layer(ssh_layer);
                }

                LayerConfig::Docker {
                    container,
                    user,
                    working_dir,
                    env: layer_env,
                    interactive,
                    tty,
                } => {
                    let mut docker_layer = DockerLayer::new(container);

                    // Apply layer-specific environment
                    for (key, value) in layer_env {
                        docker_layer = docker_layer.with_env(key, value);
                    }

                    // Apply global environment
                    for (key, value) in env {
                        docker_layer = docker_layer.with_env(key, value);
                    }

                    if let Some(u) = user {
                        docker_layer = docker_layer.with_user(u);
                    }

                    if let Some(wd) = working_dir {
                        docker_layer = docker_layer.with_working_dir(wd);
                    }

                    if *interactive {
                        docker_layer = docker_layer.with_interactive(true);
                    }

                    if *tty {
                        docker_layer = docker_layer.with_tty(true);
                    }

                    executor = executor.with_layer(docker_layer);
                }
            }
        }

        executor
    }

    /// Get the number of running processes (for testing)
    #[cfg(test)]
    pub async fn running_process_count(&self) -> usize {
        self.running_processes.lock().await.len()
    }
}

impl Default for LayeredServiceExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceExecutor for LayeredServiceExecutor {
    async fn start(
        &self,
        config: ServiceConfig,
        spawner: &dyn Spawner,
    ) -> std::result::Result<RunningService, Error> {
        let ServiceTarget::Layered {
            layers,
            command,
            command_template,
            params,
            health_check: _,
            ..
        } = &config.target
        else {
            return Err(Error::Config(
                "LayeredServiceExecutor can only handle Layered targets".to_string(),
            ));
        };

        info!(
            "Starting layered service: {} with {} layers",
            config.name,
            layers.len()
        );

        // Get environment variables from config (with substitutions if using params)
        let env = if !params.is_empty() {
            config.target.build_env()
        } else {
            config.target.env()
        };

        // Build the layered executor with all layers
        let executor = Self::build_executor(layers, &env);

        // Build command from template or legacy command
        let cmd = if let Some(template) = command_template {
            let cmd_parts = config.target.build_command().unwrap_or_default();
            if cmd_parts.is_empty() {
                return Err(Error::Config("No command built from template".to_string()));
            }
            let mut c = Command::new(&cmd_parts[0]);
            if cmd_parts.len() > 1 {
                c.args(&cmd_parts[1..]);
            }
            c
        } else if let Some(cmd_spec) = command {
            let mut c = Command::new(&cmd_spec.binary);
            for arg in &cmd_spec.args {
                c.arg(arg);
            }
            c
        } else {
            return Err(Error::Config(
                "No command specified for layered target".to_string(),
            ));
        };

        debug!("Executing layered command: {:?}", cmd);

        // Execute command through layers
        let (event_stream, handle) = executor
            .execute_command(cmd)
            .await
            .map_err(Error::CommandExecutor)?;

        // Get process PID (may not be available for remote processes)
        let pid = handle.pid();

        info!(
            "Started layered service '{}' with PID: {:?}",
            config.name, pid
        );

        // Create running service instance
        let mut running_service = RunningService::new(config.name.clone(), config.clone())
            .with_metadata("executor_type".to_string(), "layered".to_string())
            .with_metadata("layer_count".to_string(), layers.len().to_string());

        if let Some(pid) = pid {
            running_service = running_service.with_pid(pid);
        }

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

        // Store the process handle and event receiver
        {
            let mut processes = self.running_processes.lock().await;
            processes.insert(
                running_service.id.to_string(),
                LayeredProcessInfo {
                    handle: Box::new(handle) as Box<dyn ProcessHandle>,
                    event_receiver: rx,
                    layers: layers.clone(),
                },
            );
        }

        Ok(running_service)
    }

    async fn stop(
        &self,
        service: &RunningService,
        _spawner: &dyn Spawner,
    ) -> std::result::Result<(), Error> {
        info!("Stopping layered service: {}", service.name);

        // Remove and get the process info
        let process_info = {
            let mut processes = self.running_processes.lock().await;
            processes.remove(&service.id.to_string())
        };

        if let Some(mut process_info) = process_info {
            // Use the handle to properly terminate the process
            match process_info.handle.terminate().await {
                Ok(_) => {
                    info!("Successfully terminated layered service: {}", service.name);
                }
                Err(e) => {
                    warn!(
                        "Failed to terminate layered service {}: {}, trying kill",
                        service.name, e
                    );
                    // Try force kill
                    if let Err(e) = process_info.handle.kill().await {
                        warn!("Failed to kill layered service {}: {}", service.name, e);
                    }
                }
            }
        } else {
            warn!(
                "Layered service {} not found in running processes",
                service.name
            );
            return Err(Error::ServiceNotFound(service.name.clone()));
        }

        Ok(())
    }

    async fn health_check(
        &self,
        service: &RunningService,
    ) -> std::result::Result<HealthStatus, Error> {
        debug!("Health checking layered service: {}", service.name);

        let processes = self.running_processes.lock().await;

        if let Some(_process_info) = processes.get(&service.id.to_string()) {
            // Get health check from target or fall back to service config
            let health_check_config = match &service.config.target {
                ServiceTarget::Layered {
                    health_check,
                    layers,
                    ..
                } => {
                    // Prefer health check from layered target
                    if let Some(hc) = health_check {
                        // Run health check through the same layers
                        let env = std::collections::HashMap::new();
                        let executor = Self::build_executor(layers, &env);

                        let mut cmd = Command::new(&hc.command);
                        for arg in &hc.args {
                            cmd.arg(arg);
                        }

                        debug!(
                            "Running layered health check: {} {}",
                            hc.command,
                            hc.args.join(" ")
                        );

                        // Execute the health check through the layers with timeout
                        let timeout = std::time::Duration::from_secs(hc.timeout);
                        let (mut event_stream, mut handle) =
                            match executor.execute_command(cmd).await {
                                Ok(result) => result,
                                Err(e) => {
                                    debug!("Health check launch error: {}", e);
                                    return Ok(HealthStatus::Unhealthy(format!(
                                        "Health check launch error: {}",
                                        e
                                    )));
                                }
                            };

                        // Wait for the command to complete or timeout
                        let result = async {
                            use futures::StreamExt;
                            let mut exit_code = None;

                            while let Some(event) = event_stream.next().await {
                                if let command_executor::event::ProcessEventType::Exited {
                                    code,
                                    ..
                                } = event.event_type
                                {
                                    exit_code = code;
                                    break;
                                }
                            }

                            exit_code
                        };

                        // Race between timeout and completion
                        use async_runtime_compat::runtime_utils::timeout as async_timeout;
                        match async_timeout(timeout, result).await {
                            Ok(Some(0)) => {
                                debug!("Health check passed");
                                return Ok(HealthStatus::Healthy);
                            }
                            Ok(Some(code)) => {
                                debug!("Health check failed with exit code: {}", code);
                                return Ok(HealthStatus::Unhealthy(format!(
                                    "Health check failed with exit code: {}",
                                    code
                                )));
                            }
                            Ok(None) => {
                                debug!("Health check completed without exit code");
                                return Ok(HealthStatus::Unhealthy(
                                    "Health check completed without exit code".to_string(),
                                ));
                            }
                            Err(_) => {
                                // Timeout - kill the health check command (not the service)
                                let _ = handle.kill().await;
                                debug!("Health check command timed out after {}s", hc.timeout);
                                return Ok(HealthStatus::Unhealthy(format!(
                                    "Health check timed out after {}s",
                                    hc.timeout
                                )));
                            }
                        }
                    } else {
                        // Fall back to service config health check
                        service.config.health_check.as_ref()
                    }
                }
                _ => {
                    return Err(Error::Config(
                        "LayeredServiceExecutor requires Layered target".to_string(),
                    ));
                }
            };

            // Use legacy health check from service config if available
            if let Some(health_check) = health_check_config {
                return self.health_checker.check_health(health_check).await;
            } else {
                // Default: assume healthy if we can reach this point
                return Ok(HealthStatus::Healthy);
            }
        } else {
            Ok(HealthStatus::Unhealthy(
                "Layered service not found".to_string(),
            ))
        }
    }

    async fn stream_events(
        &self,
        service: &RunningService,
        _spawner: &dyn Spawner,
    ) -> std::result::Result<Receiver<ProcessEvent>, Error> {
        debug!(
            "Creating event stream for layered service: {}",
            service.name
        );

        // Get the event receiver for this service
        let processes = self.running_processes.lock().await;
        let process_info = processes
            .get(&service.id.to_string())
            .ok_or_else(|| Error::ServiceNotFound(service.name.clone()))?;

        // Clone the receiver (async-channel receivers are cloneable)
        Ok(process_info.event_receiver.clone())
    }

    fn can_handle(&self, config: &ServiceConfig) -> bool {
        matches!(config.target, ServiceTarget::Layered { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CommandSpec, ServiceConfig, ServiceTarget};

    #[test]
    fn test_can_handle() {
        let executor = LayeredServiceExecutor::new();

        // Should handle Layered targets
        let layered_config = ServiceConfig {
            name: "test-service".to_string(),
            target: ServiceTarget::Layered {
                params: HashMap::new(),
                layers: vec![LayerConfig::Ssh {
                    host: "example.com".to_string(),
                    user: "testuser".to_string(),
                    env: HashMap::new(),
                    port: None,
                    identity_file: None,
                    options: vec![],
                }],
                command_template: None,
                command: Some(CommandSpec {
                    binary: "echo".to_string(),
                    args: vec!["hello".to_string()],
                }),
                health_check: None,
            },
            depends_on: vec![],
            health_check: None,
        };

        assert!(executor.can_handle(&layered_config));

        // Should not handle Process targets
        let process_config = ServiceConfig {
            name: "test-service".to_string(),
            target: ServiceTarget::Process {
                command: crate::config::ProcessCommand::Legacy {
                    command: "echo hello".to_string(),
                },
                env: HashMap::new(),
                working_dir: None,
            },
            depends_on: vec![],
            health_check: None,
        };

        assert!(!executor.can_handle(&process_config));
    }

    #[test]
    fn test_build_executor() {
        let layers = vec![
            LayerConfig::Ssh {
                host: "jump.example.com".to_string(),
                user: "jump".to_string(),
                env: HashMap::new(),
                port: Some(2222),
                identity_file: Some("/path/to/key".to_string()),
                options: vec!["-o StrictHostKeyChecking=no".to_string()],
            },
            LayerConfig::Docker {
                container: "my-app".to_string(),
                user: Some("app".to_string()),
                working_dir: Some("/app".to_string()),
                env: HashMap::new(),
                interactive: false,
                tty: false,
            },
        ];

        let global_env = HashMap::from([("GLOBAL_VAR".to_string(), "value".to_string())]);

        // Test that build_executor doesn't panic
        let _executor = LayeredServiceExecutor::build_executor(&layers, &global_env);

        // We can't easily inspect the layers inside the executor,
        // but we've verified it builds without errors
    }

    #[smol_potat::test]
    async fn test_running_process_count() {
        let executor = LayeredServiceExecutor::new();
        assert_eq!(executor.running_process_count().await, 0);
    }
}
