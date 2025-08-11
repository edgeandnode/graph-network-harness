//! Layered executor for flexible service execution with composed layers.

use super::{
    EventStream, RunningService, ServiceExecutor,
    stream_utils::{SharedEventStream, create_forwarding_stream},
};
use crate::{
    Error,
    config::{ServiceConfig, ServiceTarget},
    health::{HealthChecker, HealthStatus},
};
use async_trait::async_trait;
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
    event_stream: SharedEventStream,
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
    async fn start(&self, config: ServiceConfig) -> std::result::Result<RunningService, Error> {
        let ServiceTarget::Layered { layers, command } = &config.target else {
            return Err(Error::Config(
                "LayeredServiceExecutor can only handle Layered targets".to_string(),
            ));
        };

        info!(
            "Starting layered service: {} with {} layers",
            config.name,
            layers.len()
        );

        // Get environment variables from config
        let env = config.target.env();

        // Build the layered executor with all layers
        let executor = Self::build_executor(layers, &env);

        // Build command
        let mut cmd = Command::new(&command.binary);
        for arg in &command.args {
            cmd.arg(arg);
        }

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

        // Store the process handle and event stream
        {
            let mut processes = self.running_processes.lock().await;
            processes.insert(
                running_service.id.to_string(),
                LayeredProcessInfo {
                    handle: Box::new(handle) as Box<dyn ProcessHandle>,
                    event_stream: Arc::new(Mutex::new(Box::new(event_stream))),
                    layers: layers.clone(),
                },
            );
        }

        Ok(running_service)
    }

    async fn stop(&self, service: &RunningService) -> std::result::Result<(), Error> {
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
            // Use configured health check if available
            if let Some(health_check) = &service.config.health_check {
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
    ) -> std::result::Result<EventStream, Error> {
        debug!(
            "Creating event stream for layered service: {}",
            service.name
        );

        // Get the event stream for this service
        let processes = self.running_processes.lock().await;
        let process_info = processes
            .get(&service.id.to_string())
            .ok_or_else(|| Error::ServiceNotFound(service.name.clone()))?;

        let event_stream = process_info.event_stream.clone();
        drop(processes); // Release the lock early

        Ok(create_forwarding_stream(event_stream))
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
                layers: vec![LayerConfig::Ssh {
                    host: "example.com".to_string(),
                    user: "testuser".to_string(),
                    env: HashMap::new(),
                    port: None,
                    identity_file: None,
                    options: vec![],
                }],
                command: CommandSpec {
                    binary: "echo".to_string(),
                    args: vec!["hello".to_string()],
                },
            },
            dependencies: vec![],
            health_check: None,
        };

        assert!(executor.can_handle(&layered_config));

        // Should not handle Process targets
        let process_config = ServiceConfig {
            name: "test-service".to_string(),
            target: ServiceTarget::Process {
                binary: "echo".to_string(),
                args: vec!["hello".to_string()],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![],
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
