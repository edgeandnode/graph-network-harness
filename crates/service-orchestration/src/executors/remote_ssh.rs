//! Remote SSH executor for distributed service execution.

use super::{
    EventStream, RunningService, ServiceExecutor,
};
use crate::{
    Error,
    config::{ServiceConfig, ServiceTarget, RemoteMode},
    health::{HealthChecker, HealthStatus},
};
use async_trait::async_trait;
use command_executor::{
    Command, ProcessHandle, backends::LocalLauncher,
    layered::{LayeredExecutor, SshLayer},
};
use futures::lock::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Information about a running remote SSH process
struct RemoteSshInfo {
    handle: Box<dyn ProcessHandle>,
    host: String,
    user: String,
}

/// Executor for remote SSH service execution
///
/// Uses interior mutability via Arc<Mutex<>> for running_processes to allow
/// modification through &self methods as required by the ServiceExecutor trait.
pub struct RemoteSshExecutor {
    health_checker: HealthChecker,
    running_processes: Arc<Mutex<HashMap<String, RemoteSshInfo>>>,
}

impl RemoteSshExecutor {
    /// Create a new remote SSH executor
    pub fn new() -> Self {
        Self {
            health_checker: HealthChecker::new(),
            running_processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get the number of running processes (for testing)
    #[cfg(test)]
    pub async fn running_process_count(&self) -> usize {
        self.running_processes.lock().await.len()
    }

    /// Create SSH layer from remote configuration with authentication support
    fn create_ssh_layer(host: &str, user: &str, env: &HashMap<String, String>) -> SshLayer {
        let destination = format!("{user}@{host}");
        let mut ssh_layer = SshLayer::new(destination);

        // Add environment variables
        for (key, value) in env {
            ssh_layer = ssh_layer.with_env(key, value);
        }

        // Enable agent forwarding by default for SSH key authentication
        ssh_layer = ssh_layer.with_agent_forwarding(true);

        // Check for SSH identity file in environment variables
        if let Some(identity_file) = env.get("SSH_IDENTITY_FILE") {
            ssh_layer = ssh_layer.with_identity_file(identity_file);
        } else if let Some(identity_file) = env.get("SSH_KEY_PATH") {
            ssh_layer = ssh_layer.with_identity_file(identity_file);
        }

        // Check for SSH port override
        if let Some(port_str) = env.get("SSH_PORT") {
            if let Ok(port) = port_str.parse::<u16>() {
                ssh_layer = ssh_layer.with_port(port);
            }
        }

        // Add any custom SSH options
        if let Some(options_str) = env.get("SSH_OPTIONS") {
            for option in options_str.split(',') {
                ssh_layer = ssh_layer.with_option(option.trim());
            }
        }

        ssh_layer
    }

    /// Build command from remote mode configuration
    fn build_command(mode: &RemoteMode) -> Command {
        match mode {
            RemoteMode::Process { binary, args } => {
                let mut command = Command::new(binary);
                for arg in args {
                    command.arg(arg);
                }
                command
            }
        }
    }
}

impl Default for RemoteSshExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceExecutor for RemoteSshExecutor {
    async fn start(&self, config: ServiceConfig) -> std::result::Result<RunningService, Error> {
        let ServiceTarget::Remote { host, user, mode, env } = &config.target else {
            return Err(Error::Config("RemoteSshExecutor can only handle Remote targets".to_string()));
        };

        info!("Starting remote SSH service: {} on {}@{}", config.name, user, host);

        // Create SSH layer
        let ssh_layer = Self::create_ssh_layer(host, user, env);
        
        // Create layered executor with SSH
        let executor = LayeredExecutor::new(LocalLauncher)
            .with_layer(ssh_layer);

        // Build command
        let command = Self::build_command(mode);

        debug!("Executing SSH command: {:?}", command);

        // Execute command through SSH
        let (event_stream_pin, handle) = executor.execute_command(command).await.map_err(|e| {
            Error::CommandExecutor(e)
        })?;

        // Get process PID (may not be available for SSH processes)
        let pid = handle.pid();

        // Store process info
        let remote_info = RemoteSshInfo {
            handle: Box::new(handle),
            host: host.clone(),
            user: user.clone(),
        };

        {
            let mut processes = self.running_processes.lock().await;
            processes.insert(config.name.clone(), remote_info);
        }

        // Create running service
        let mut running_service = RunningService::new(config.name.clone(), config.clone())
            .with_metadata("executor_type".to_string(), "remote-ssh".to_string())
            .with_metadata("ssh_host".to_string(), host.clone())
            .with_metadata("ssh_user".to_string(), user.clone());

        if let Some(pid) = pid {
            running_service = running_service.with_pid(pid);
        }

        info!("Successfully started remote SSH service: {}", running_service.name);

        Ok(running_service)
    }

    async fn stop(&self, service: &RunningService) -> std::result::Result<(), Error> {
        info!("Stopping remote SSH service: {}", service.name);

        let mut processes = self.running_processes.lock().await;
        
        if let Some(mut remote_info) = processes.remove(&service.name) {
            // Attempt to terminate the remote process
            if let Err(e) = remote_info.handle.terminate().await {
                warn!("Failed to terminate remote SSH process {}: {}", service.name, e);
                // Continue with forceful kill
            }

            // Wait for process to exit
            match remote_info.handle.wait().await {
                Ok(exit_status) => {
                    info!("Remote SSH service {} exited with status: {:?}", service.name, exit_status);
                }
                Err(e) => {
                    warn!("Error waiting for remote SSH service {} to exit: {}", service.name, e);
                }
            }

            info!("Successfully stopped remote SSH service: {}", service.name);
            Ok(())
        } else {
            warn!("Remote SSH service {} not found in running processes", service.name);
            Err(Error::ServiceNotFound(service.name.clone()))
        }
    }

    async fn health_check(
        &self,
        service: &RunningService,
    ) -> std::result::Result<HealthStatus, Error> {
        debug!("Health checking remote SSH service: {}", service.name);

        let processes = self.running_processes.lock().await;
        
        if let Some(remote_info) = processes.get(&service.name) {
            // For health check, assume healthy if the process hasn't exited yet
            // SSH processes are tricky to check without actually waiting
            if let Some(health_check) = &service.config.health_check {
                // Use configured health check
                return self.health_checker.check_health(health_check).await;
            } else {
                // Default: assume healthy if we can reach this point
                return Ok(HealthStatus::Healthy);
            }
        } else {
            Ok(HealthStatus::Unhealthy("Remote SSH service not found".to_string()))
        }
    }

    async fn stream_events(
        &self,
        service: &RunningService,
    ) -> std::result::Result<EventStream, Error> {
        debug!("Creating event stream for remote SSH service: {}", service.name);

        let processes = self.running_processes.lock().await;
        
        if let Some(_remote_info) = processes.get(&service.name) {
            // For now, return an empty stream since we can't easily store the original
            // Pin<Box<dyn Stream>> in our struct. This can be improved later.
            use futures::stream;
            Ok(Box::pin(stream::empty()))
        } else {
            Err(Error::ServiceNotFound(service.name.clone()))
        }
    }

    fn can_handle(&self, config: &ServiceConfig) -> bool {
        matches!(config.target, ServiceTarget::Remote { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ServiceConfig, ServiceTarget, RemoteMode};
    use std::collections::HashMap;

    #[test]
    fn test_can_handle() {
        let executor = RemoteSshExecutor::new();

        // Should handle Remote targets
        let remote_config = ServiceConfig {
            name: "test-service".to_string(),
            target: ServiceTarget::Remote {
                host: "example.com".to_string(),
                user: "testuser".to_string(),
                mode: RemoteMode::Process {
                    binary: "echo".to_string(),
                    args: vec!["hello".to_string()],
                },
                env: HashMap::new(),
            },
            dependencies: vec![],
            health_check: None,
        };

        assert!(executor.can_handle(&remote_config));

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
    fn test_build_command() {
        let mode = RemoteMode::Process {
            binary: "nginx".to_string(),
            args: vec!["-g".to_string(), "daemon off;".to_string()],
        };

        let command = RemoteSshExecutor::build_command(&mode);
        
        // Note: We can't easily test the internal Command structure,
        // but we can verify the method doesn't panic
        assert!(!command.get_program().is_empty());
    }

    #[test] 
    fn test_create_ssh_layer() {
        let mut env = HashMap::new();
        env.insert("NODE_ENV".to_string(), "production".to_string());

        let ssh_layer = RemoteSshExecutor::create_ssh_layer("example.com", "deploy", &env);

        assert_eq!(ssh_layer.destination, "deploy@example.com");
        assert_eq!(ssh_layer.env.get("NODE_ENV"), Some(&"production".to_string()));
        assert!(ssh_layer.agent_forwarding);
    }

    #[test]
    fn test_create_ssh_layer_with_authentication() {
        let mut env = HashMap::new();
        env.insert("SSH_IDENTITY_FILE".to_string(), "/home/user/.ssh/id_rsa".to_string());
        env.insert("SSH_PORT".to_string(), "2222".to_string());
        env.insert("SSH_OPTIONS".to_string(), "-o StrictHostKeyChecking=no,-o UserKnownHostsFile=/dev/null".to_string());

        let ssh_layer = RemoteSshExecutor::create_ssh_layer("remote.example.com", "admin", &env);

        assert_eq!(ssh_layer.destination, "admin@remote.example.com");
        assert_eq!(ssh_layer.identity_file, Some(std::path::PathBuf::from("/home/user/.ssh/id_rsa")));
        assert_eq!(ssh_layer.port, Some(2222));
        assert!(ssh_layer.agent_forwarding);
        assert_eq!(ssh_layer.options.len(), 2);
        assert!(ssh_layer.options.contains(&"-o StrictHostKeyChecking=no".to_string()));
        assert!(ssh_layer.options.contains(&"-o UserKnownHostsFile=/dev/null".to_string()));
    }

    #[smol_potat::test]
    async fn test_running_process_count() {
        let executor = RemoteSshExecutor::new();
        assert_eq!(executor.running_process_count().await, 0);
    }
}