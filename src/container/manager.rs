use anyhow::{Context, Result};
use bollard::container::{ListContainersOptions, StartContainerOptions};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::models::ContainerSummary;
use bollard::Docker;
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use tracing::{debug, info};

use super::config::ContainerConfig;
use super::image_sync::ImageSync;

// Constants for paths
const LOCAL_NETWORK_MOUNT_PATH: &str = "/local-network";
const DOCKER_COMPOSE_FILE: &str = "docker-compose.yaml";

/// Manages the Docker-in-Docker container lifecycle
pub struct DindManager {
    docker: Docker,
    config: ContainerConfig,
    container_id: Option<String>,
    session_id: String,
    session_start: DateTime<Utc>,
}

impl DindManager {
    /// Create a new DindManager
    pub fn new(config: ContainerConfig) -> Result<Self> {
        let docker = Docker::connect_with_local_defaults()
            .context("Failed to connect to Docker")?;
        
        let now = Utc::now();
        // Create session ID with consistent formatting
        let session_id = now.format("%Y-%m-%d_%H-%M-%S").to_string();
        
        Ok(Self {
            docker,
            config,
            container_id: None,
            session_id,
            session_start: now,
        })
    }
    
    /// Get the session ID for this manager
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    
    /// Format a timestamp for log entries
    fn format_timestamp(dt: &DateTime<Utc>) -> String {
        dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
    }
    
    /// Get the log file path for this session
    fn get_log_path(&self, name: &str) -> PathBuf {
        self.config.log_dir.join(format!("{}_{}.log", self.session_id, name))
    }
    
    /// Ensure log directory exists
    pub async fn ensure_log_dir(&self) -> Result<()> {
        tokio::fs::create_dir_all(&self.config.log_dir)
            .await
            .context("Failed to create log directory")?;
        Ok(())
    }
    
    /// Ensure the DinD container is running
    pub async fn ensure_running(&mut self) -> Result<()> {
        // First check if container exists and is running
        if let Some(container) = self.find_container().await? {
            if container.state.as_deref() == Some("running") {
                info!("DinD container is already running");
                self.container_id = container.id;
                return Ok(());
            }
            
            // Container exists but not running, start it
            if let Some(id) = &container.id {
                info!("Starting existing DinD container");
                self.docker
                    .start_container(id, None::<StartContainerOptions<String>>)
                    .await
                    .context("Failed to start container")?;
                self.container_id = Some(id.clone());
            }
        } else {
            // Container doesn't exist, use docker-compose to create it
            info!("DinD container not found, creating with docker-compose");
            self.start_with_compose().await?;
        }
        
        // Wait for Docker daemon to be ready inside container
        self.wait_for_docker_ready().await?;
        
        Ok(())
    }
    
    /// Find the DinD container
    async fn find_container(&self) -> Result<Option<ContainerSummary>> {
        // Docker compose creates container names like "docker-test-env-integration-tests-dind-1"
        // We need to search for containers that contain our service name
        let options = ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        };
        
        let containers = self.docker
            .list_containers(Some(options))
            .await
            .context("Failed to list containers")?;
        
        // Look for a container whose name contains "integration-tests-dind"
        let container = containers.into_iter().find(|c| {
            if let Some(names) = &c.names {
                names.iter().any(|name| name.contains("integration-tests-dind"))
            } else {
                false
            }
        });
        
        if let Some(ref c) = container {
            info!("Found DinD container: {:?}", c.names);
        }
        
        Ok(container)
    }
    
    /// Start the container using docker-compose
    async fn start_with_compose(&mut self) -> Result<()> {
        info!("Starting docker-compose from directory: {:?}", self.config.docker_test_env_path);
        
        // Validate docker-test-env path exists
        if !self.config.docker_test_env_path.exists() {
            anyhow::bail!("docker-test-env path does not exist: {:?}", self.config.docker_test_env_path);
        }
        
        // Check for docker-compose.yaml
        let compose_file = self.config.docker_test_env_path.join("docker-compose.yaml");
        if !compose_file.exists() {
            // Also check for docker-compose.yml
            let compose_yml = self.config.docker_test_env_path.join("docker-compose.yml");
            if compose_yml.exists() {
                anyhow::bail!(
                    "Found docker-compose.yml but expected docker-compose.yaml at: {:?}",
                    self.config.docker_test_env_path
                );
            }
            anyhow::bail!(
                "docker-compose.yaml not found at: {:?}\nDirectory contents: {:?}",
                compose_file,
                std::fs::read_dir(&self.config.docker_test_env_path)?
                    .filter_map(|e| e.ok())
                    .map(|e| e.file_name())
                    .collect::<Vec<_>>()
            );
        }
        
        // Ensure log directory exists
        self.ensure_log_dir().await?;
        
        // Create log file for docker-compose
        let log_path = self.get_log_path("docker-compose");
        let log_file = Arc::new(Mutex::new(BufWriter::new(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .await
                .context("Failed to create docker-compose log file")?
        )));
        
        info!("Starting container with docker-compose, logging to: {:?}", log_path);
        
        // Run docker-compose up -d
        // Use the compose file name only since we're changing to the directory
        let mut child = Command::new("docker-compose")
            .arg("-f")
            .arg("docker-compose.yaml")
            .arg("up")
            .arg("-d")
            .current_dir(&self.config.docker_test_env_path)
            .env("LOCAL_NETWORK_PATH", &self.config.local_network_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("Failed to spawn docker-compose")?;
        
        // Stream stdout
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        
        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();
        
        // Spawn tasks to handle stdout and stderr
        let log_file_clone = Arc::clone(&log_file);
        let stdout_task = tokio::spawn(async move {
            while let Some(line) = stdout_reader.next_line().await? {
                let timestamp = Self::format_timestamp(&Utc::now());
                let log_line = format!("[{}] [stdout] {}\n", timestamp, line);
                let mut writer = log_file_clone.lock().await;
                writer.write_all(log_line.as_bytes()).await?;
                writer.flush().await?;
                debug!("[docker-compose] {}", line);
            }
            Ok::<(), anyhow::Error>(())
        });
        
        let stderr_task = tokio::spawn(async move {
            while let Some(line) = stderr_reader.next_line().await? {
                let timestamp = Self::format_timestamp(&Utc::now());
                let log_line = format!("[{}] [stderr] {}\n", timestamp, line);
                let mut writer = log_file.lock().await;
                writer.write_all(log_line.as_bytes()).await?;
                writer.flush().await?;
                debug!("[docker-compose] {}", line);
            }
            Ok::<(), anyhow::Error>(())
        });
        
        // Wait for process to complete
        let status = child.wait().await?;
        
        // Wait for log tasks
        stdout_task.await??;
        stderr_task.await??;
        
        if !status.success() {
            anyhow::bail!("docker-compose failed with status: {}", status);
        }
        
        // Wait a moment for container to be registered
        sleep(Duration::from_secs(2)).await;
        
        // Find the container ID
        if let Some(container) = self.find_container().await? {
            self.container_id = container.id;
        } else {
            anyhow::bail!("Container not found after docker-compose up");
        }
        
        Ok(())
    }
    
    /// Wait for Docker daemon to be ready inside the container
    async fn wait_for_docker_ready(&self) -> Result<()> {
        let _container_id = self.container_id.as_ref()
            .context("No container ID set")?;
        
        info!("Waiting for Docker daemon to be ready inside container...");
        
        let start = std::time::Instant::now();
        let timeout = self.config.docker_ready_timeout;
        
        loop {
            if start.elapsed() > timeout {
                anyhow::bail!("Timeout waiting for Docker daemon to be ready");
            }
            
            // Try to run docker version inside the container
            match self.exec_simple(vec!["docker", "version"]).await {
                Ok(_) => {
                    info!("Docker daemon is ready!");
                    return Ok(());
                }
                Err(_) => {
                    debug!("Docker daemon not ready yet, retrying...");
                    sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }
    
    /// Execute a simple command in the container and return output
    pub async fn exec_simple(&self, cmd: Vec<&str>) -> Result<String> {
        let container_id = self.container_id.as_ref()
            .context("No container ID set")?;
        
        let exec = self.docker
            .create_exec(
                container_id,
                CreateExecOptions {
                    cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .context("Failed to create exec")?;
        
        let start_exec = self.docker
            .start_exec(&exec.id, None)
            .await
            .context("Failed to start exec")?;
        
        match start_exec {
            StartExecResults::Attached { mut output, .. } => {
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                
                while let Some(msg) = output.next().await {
                    match msg {
                        Ok(bollard::container::LogOutput::StdOut { message }) => {
                            stdout.extend_from_slice(&message);
                        }
                        Ok(bollard::container::LogOutput::StdErr { message }) => {
                            stderr.extend_from_slice(&message);
                        }
                        _ => {}
                    }
                }
                
                if !stderr.is_empty() {
                    let stderr_str = String::from_utf8_lossy(&stderr);
                    debug!("Command stderr: {}", stderr_str);
                }
                
                Ok(String::from_utf8_lossy(&stdout).to_string())
            }
            _ => anyhow::bail!("Unexpected exec result"),
        }
    }
    
    /// Execute a command in the container with real-time output
    pub async fn exec_in_container(&self, cmd: Vec<&str>, working_dir: Option<&str>) -> Result<i64> {
        let container_id = self.container_id.as_ref()
            .context("No container ID set")?;
        
        // Create a log file for this command
        let cmd_name = cmd.first().unwrap_or(&"unknown").replace("/", "_");
        let log_path = self.get_log_path(&format!("exec_{}", cmd_name));
        let mut log_file = BufWriter::new(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .await
                .context("Failed to create exec log file")?
        );
        
        info!("Executing in container: {:?}, logging to: {:?}", cmd, log_path);
        
        let mut exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            attach_stdin: Some(false),
            tty: Some(true),
            env: Some(vec![
                "CARGO_TERM_COLOR=never".to_string(),
            ]),
            ..Default::default()
        };
        
        if let Some(wd) = working_dir {
            exec_options.working_dir = Some(wd.to_string());
        }
        
        let exec = self.docker
            .create_exec(container_id, exec_options)
            .await
            .context("Failed to create exec")?;
        
        let start_exec = self.docker
            .start_exec(&exec.id, None)
            .await
            .context("Failed to start exec")?;
        
        // Stream output to console and log file
        match start_exec {
            StartExecResults::Attached { mut output, .. } => {
                while let Some(msg) = output.next().await {
                    match msg {
                        Ok(bollard::container::LogOutput::StdOut { message }) => {
                            let text = String::from_utf8_lossy(&message);
                            print!("{}", text);
                            std::io::Write::flush(&mut std::io::stdout())?;
                            
                            let timestamp = Self::format_timestamp(&Utc::now());
                            let log_line = format!("[{}] [stdout] {}", timestamp, text);
                            log_file.write_all(log_line.as_bytes()).await?;
                            log_file.flush().await?;
                        }
                        Ok(bollard::container::LogOutput::StdErr { message }) => {
                            let text = String::from_utf8_lossy(&message);
                            eprint!("{}", text);
                            std::io::Write::flush(&mut std::io::stderr())?;
                            
                            let timestamp = Self::format_timestamp(&Utc::now());
                            let log_line = format!("[{}] [stderr] {}", timestamp, text);
                            log_file.write_all(log_line.as_bytes()).await?;
                            log_file.flush().await?;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        
        // Get exit code
        let inspect = self.docker
            .inspect_exec(&exec.id)
            .await
            .context("Failed to inspect exec")?;
        
        let exit_code = inspect.exit_code.unwrap_or(0);
        let timestamp = Self::format_timestamp(&Utc::now());
        let log_line = format!("[{}] Process exited with code: {}\n", timestamp, exit_code);
        log_file.write_all(log_line.as_bytes()).await?;
        log_file.flush().await?;
        
        Ok(exit_code)
    }
    
    /// Build the integration tests inside the container
    pub async fn build_in_container(&self) -> Result<()> {
        info!("Building integration tests inside container...");
        
        let exit_code = self.exec_in_container(
            vec!["cargo", "build", "--color=never", "--bin", "integration-tests"],
            Some("/workspace"),
        ).await?;
        
        if exit_code != 0 {
            anyhow::bail!("Build failed with exit code: {}", exit_code);
        }
        
        Ok(())
    }
    
    /// Run the integration tests with specified command
    pub async fn run_tests(&self, command: Vec<&str>) -> Result<i64> {
        let mut args = vec!["./target/debug/integration-tests"];
        args.extend(command);
        
        info!("Running command: {}", args.join(" "));
        
        self.exec_in_container(args, Some("/workspace")).await
    }
    
    /// Sync images from host to DinD container
    pub async fn sync_images(&self) -> Result<crate::container::image_sync::ImageSyncResult> {
        let container_id = self.container_id.as_ref()
            .context("No container ID set")?;
        
        // Always use the configured local_network_path - no magic detection
        let compose_file_path = self.config.local_network_path.join(DOCKER_COMPOSE_FILE);
        
        let image_sync = ImageSync::new(
            self.docker.clone(),
            container_id.clone(),
            compose_file_path,
        );
        
        image_sync.sync_all().await
    }
    
    /// Build images on the host using build-with-overrides.sh
    pub async fn build_host_images(&self) -> Result<()> {
        
        let local_network_path = &self.config.local_network_path;
        let build_script = local_network_path.join("scripts/build-with-overrides.sh");
        
        if !build_script.exists() {
            anyhow::bail!("Build script not found at: {:?}", build_script);
        }
        
        // Ensure log directory exists
        self.ensure_log_dir().await?;
        
        // Create log file for build process
        let log_path = self.get_log_path("build-images");
        let log_file = Arc::new(Mutex::new(BufWriter::new(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .await
                .context("Failed to create build log file")?
        )));
        
        info!("Building images on host using build-with-overrides.sh, logging to: {:?}", log_path);
        
        // Set INDEXER_AGENT_SOURCE_ROOT to the indexer submodule
        let indexer_source = self.config.project_root.join("submodules/indexer");
        
        let mut child = Command::new("bash")
            .arg(build_script.to_str().unwrap())
            .current_dir(&local_network_path)
            .env("INDEXER_AGENT_SOURCE_ROOT", indexer_source.to_string_lossy().to_string())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("Failed to spawn build-with-overrides.sh")?;
        
        // Stream stdout and stderr
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        
        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();
        
        // Spawn tasks to handle stdout and stderr
        let log_file_clone = Arc::clone(&log_file);
        let stdout_task = tokio::spawn(async move {
            while let Some(line) = stdout_reader.next_line().await? {
                let timestamp = Self::format_timestamp(&Utc::now());
                let log_line = format!("[{}] [stdout] {}\n", timestamp, line);
                let mut writer = log_file_clone.lock().await;
                writer.write_all(log_line.as_bytes()).await?;
                writer.flush().await?;
                info!("[build] {}", line);
            }
            Ok::<(), anyhow::Error>(())
        });
        
        let stderr_task = tokio::spawn(async move {
            while let Some(line) = stderr_reader.next_line().await? {
                let timestamp = Self::format_timestamp(&Utc::now());
                let log_line = format!("[{}] [stderr] {}\n", timestamp, line);
                let mut writer = log_file.lock().await;
                writer.write_all(log_line.as_bytes()).await?;
                writer.flush().await?;
                debug!("[build] {}", line);
            }
            Ok::<(), anyhow::Error>(())
        });
        
        // Wait for process to complete
        let status = child.wait().await?;
        
        // Wait for log tasks
        stdout_task.await??;
        stderr_task.await??;
        
        if !status.success() {
            anyhow::bail!("Build failed with status: {}", status);
        }
        
        info!("Images built successfully on host");
        
        Ok(())
    }
    
    /// Stop the container
    pub async fn stop(&self) -> Result<()> {
        if let Some(id) = &self.container_id {
            info!("Stopping DinD container");
            self.docker
                .stop_container(id, None)
                .await
                .context("Failed to stop container")?;
        }
        Ok(())
    }
    
    /// Print a summary of log locations
    pub fn print_log_summary(&self) {
        info!("Container session logs saved to: {}", self.config.log_dir.display());
        info!("Session ID: {}", self.session_id);
        info!("Log files:");
        info!("  - docker-compose.log: Container startup logs");
        info!("  - exec_cargo.log: Cargo build logs");
        info!("  - exec_..target_debug_integration-tests.log: Test execution logs");
        if self.config.log_dir.join(format!("{}_build-images.log", self.session_id)).exists() {
            info!("  - build-images.log: Docker image build logs");
        }
    }
    
    
    /// Get the session start time for testing
    #[cfg(test)]
    pub fn session_start(&self) -> &DateTime<Utc> {
        &self.session_start
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use tempfile::TempDir;

    #[test]
    fn test_dind_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = ContainerConfig::new(
            temp_dir.path().to_path_buf(),
            temp_dir.path().to_path_buf(),
            temp_dir.path().to_path_buf(),
        );
        
        let manager = DindManager::new(config);
        assert!(manager.is_ok());
        
        let manager = manager.unwrap();
        assert!(manager.container_id.is_none());
        assert!(!manager.session_id.is_empty());
    }

    #[test]
    fn test_log_path_generation() {
        let temp_dir = TempDir::new().unwrap();
        let config = ContainerConfig::new(
            temp_dir.path().to_path_buf(),
            temp_dir.path().to_path_buf(),
            temp_dir.path().to_path_buf(),
        );
        
        let manager = DindManager::new(config).unwrap();
        let log_path = manager.get_log_path("test");
        
        assert!(log_path.to_string_lossy().contains(&manager.session_id));
        assert!(log_path.to_string_lossy().ends_with("_test.log"));
    }

    #[tokio::test]
    async fn test_ensure_log_dir() {
        let temp_dir = TempDir::new().unwrap();
        let log_dir = temp_dir.path().join("logs/test");
        
        let config = ContainerConfig {
            log_dir: log_dir.clone(),
            ..ContainerConfig::default()
        };
        
        let manager = DindManager::new(config).unwrap();
        assert!(!log_dir.exists());
        
        manager.ensure_log_dir().await.unwrap();
        assert!(log_dir.exists());
    }

    #[test]
    fn test_session_id_format() {
        let config = ContainerConfig::default();
        let manager = DindManager::new(config).unwrap();
        
        // Session ID should be in format: YYYY-MM-DD_HH-MM-SS
        let session_id = manager.session_id();
        assert_eq!(session_id.len(), 19);
        assert!(session_id.chars().nth(4).unwrap() == '-');
        assert!(session_id.chars().nth(7).unwrap() == '-');
        assert!(session_id.chars().nth(10).unwrap() == '_');
        assert!(session_id.chars().nth(13).unwrap() == '-');
        assert!(session_id.chars().nth(16).unwrap() == '-');
        
        // Verify it can be parsed back
        let parsed = DateTime::parse_from_str(&format!("{} +0000", session_id.replace('_', " ")), "%Y-%m-%d %H-%M-%S %z");
        assert!(parsed.is_ok());
    }
    
    #[test]
    fn test_timestamp_formatting() {
        use chrono::Timelike;
        
        let dt = Utc.with_ymd_and_hms(2025, 1, 15, 14, 30, 45).unwrap();
        // Create a new datetime with nanoseconds for millisecond precision
        let dt_with_millis = dt.with_nanosecond(123_456_789).unwrap();
        
        let formatted = DindManager::format_timestamp(&dt_with_millis);
        assert_eq!(formatted, "2025-01-15 14:30:45.123");
    }
    
    #[test]
    fn test_session_tracking() {
        let config = ContainerConfig::default();
        let before = Utc::now();
        let manager = DindManager::new(config).unwrap();
        let after = Utc::now();
        
        // Session start should be between before and after
        let session_start = manager.session_start();
        assert!(session_start >= &before);
        assert!(session_start <= &after);
        
        // Session ID should match the session start time
        let expected_id = session_start.format("%Y-%m-%d_%H-%M-%S").to_string();
        assert_eq!(manager.session_id(), expected_id);
    }
}