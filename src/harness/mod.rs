//! High-level test harness for local-network integration testing
//! 
//! This module provides the main API for running integration tests against
//! a Graph Protocol local network deployment.

use crate::container::{ContainerConfig, DindManager};
use crate::inspection::{ServiceInspector, ServiceEventRegistry, PostgresEventHandler, GraphNodeEventHandler};
use anyhow::{Context, Result};
use bollard::Docker;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tracing::{info, warn};

// Constants for common paths and files
const LOCAL_NETWORK_MOUNT_PATH: &str = "/local-network";
const WORKSPACE_PATH: &str = "/workspace";
const DOCKER_COMPOSE_FILE: &str = "docker-compose.yaml";
const DOCKER_TEST_ENV: &str = "docker-test-env";
const INTEGRATION_TESTS: &str = "local-network-harness";
const TEST_ACTIVITY: &str = "test-activity";
const LOGS: &str = "logs";

/// Configuration for the local network test harness
#[derive(Debug, Clone)]
pub struct HarnessConfig {
    /// Path to the docker-test-env directory
    /// If not specified, will look for integration-tests/docker-test-env
    pub docker_test_env_path: Option<PathBuf>,
    
    /// Path to the local-network directory containing docker-compose.yaml
    pub local_network_path: PathBuf,
    
    /// Path to the project root
    /// Defaults to current directory
    pub project_root: PathBuf,
    
    /// Directory for storing test logs
    /// If not specified, a temporary directory will be created
    pub log_dir: Option<PathBuf>,
    
    /// Timeout for container startup
    pub startup_timeout: Duration,
    
    /// Whether to automatically sync Docker images
    pub auto_sync_images: bool,
    
    /// Whether to build local-network images before running
    pub build_images: bool,
    
    /// Test session name (for log organization)
    pub session_name: Option<String>,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            docker_test_env_path: None,
            // NOTE: local_network_path must be explicitly set - no default path
            local_network_path: PathBuf::new(), // Empty path - will require explicit setting
            project_root: current_dir,
            log_dir: None,
            startup_timeout: Duration::from_secs(60),
            auto_sync_images: true,
            build_images: false,
            session_name: None,
        }
    }
}

/// Main test harness for local-network integration testing
pub struct LocalNetworkHarness {
    config: HarnessConfig,
    container_manager: DindManager,
    _temp_dir: Option<TempDir>,
    sync_result: Option<crate::container::image_sync::ImageSyncResult>,
}

impl LocalNetworkHarness {
    /// Create a new test harness with the given configuration
    pub fn new(mut config: HarnessConfig) -> Result<Self> {
        // First, determine the actual project root
        let current_dir = std::env::current_dir().context("Failed to get current directory")?;
        let current_dir = current_dir.canonicalize()
            .context("Failed to canonicalize current directory")?;
        info!("Current directory: {:?}", current_dir);
        
        // Check if we're in the integration-tests directory
        if current_dir.ends_with(INTEGRATION_TESTS) {
            // We're in integration-tests, so project root is the parent
            if let Some(parent) = current_dir.parent() {
                config.project_root = parent.to_path_buf();
                info!("Detected running from integration-tests, setting project root to: {:?}", config.project_root);
            }
        }
        
        // Canonicalize the project root
        config.project_root = config.project_root.canonicalize()
            .context("Failed to canonicalize project root")?;
        info!("Project root (canonical): {:?}", config.project_root);
        
        // Validate local_network_path
        if config.local_network_path.as_os_str().is_empty() {
            return Err(anyhow::anyhow!(
                "local_network_path must be specified. Please provide the path to your local-network directory \
                (e.g., --local-network submodules/local-network)"
            ));
        }
        
        // Make local_network_path absolute if it's relative
        if !config.local_network_path.is_absolute() {
            config.local_network_path = config.project_root.join(&config.local_network_path);
        }
        
        // Verify local_network_path exists
        if !config.local_network_path.exists() {
            return Err(anyhow::anyhow!(
                "local_network_path does not exist: {:?}. Please ensure the path is correct.",
                config.local_network_path
            ));
        }
        
        // Verify it contains docker-compose.yaml
        let compose_file = config.local_network_path.join(DOCKER_COMPOSE_FILE);
        if !compose_file.exists() {
            return Err(anyhow::anyhow!(
                "No docker-compose.yaml found in local_network_path: {:?}",
                config.local_network_path
            ));
        }
        
        info!("Using local-network at: {:?}", config.local_network_path);
        
        // Set up docker-test-env path if not specified
        if config.docker_test_env_path.is_none() {
            // First try relative to current directory
            let docker_env = current_dir.join(DOCKER_TEST_ENV);
            if docker_env.exists() {
                let docker_env = docker_env.canonicalize()
                    .context("Failed to canonicalize docker-test-env path")?;
                info!("Found docker-test-env at: {:?}", docker_env);
                config.docker_test_env_path = Some(docker_env);
            } else {
                // Then try relative to project root
                let docker_env = config.project_root
                    .join(INTEGRATION_TESTS)
                    .join(DOCKER_TEST_ENV);
                if !docker_env.exists() {
                    return Err(anyhow::anyhow!(
                        "docker-test-env not found. Tried:\n  - {:?}\n  - {:?}\nPlease specify docker_test_env_path",
                        current_dir.join(DOCKER_TEST_ENV),
                        docker_env
                    ));
                }
                let docker_env = docker_env.canonicalize()
                    .context("Failed to canonicalize docker-test-env path")?;
                info!("Found docker-test-env at: {:?}", docker_env);
                config.docker_test_env_path = Some(docker_env);
            }
        } else {
            // Canonicalize the provided path
            let docker_env_path = config.docker_test_env_path.as_ref().unwrap();
            if !docker_env_path.exists() {
                return Err(anyhow::anyhow!(
                    "Specified docker_test_env_path does not exist: {:?}",
                    docker_env_path
                ));
            }
            config.docker_test_env_path = Some(docker_env_path.canonicalize()
                .context("Failed to canonicalize docker_test_env_path")?);
        }
        
        // Set up test activity directory
        // If we're already in integration-tests, don't add it again
        let test_activity_dir = if config.project_root.ends_with(INTEGRATION_TESTS) {
            config.project_root.join(TEST_ACTIVITY)
        } else {
            config.project_root
                .join(INTEGRATION_TESTS)
                .join(TEST_ACTIVITY)
        };
        
        // Set up log directory
        let (_temp_dir, log_dir) = if let Some(log_dir) = config.log_dir.clone() {
            std::fs::create_dir_all(&log_dir)
                .context("Failed to create log directory")?;
            (None, log_dir)
        } else {
            // Use logs subdirectory in test-activity
            let log_dir = test_activity_dir.join(LOGS);
            std::fs::create_dir_all(&log_dir)
                .context("Failed to create log directory")?;
            (None, log_dir)
        };
        
        // Create container configuration
        info!("Creating container configuration:");
        info!("  docker_test_env_path: {:?}", config.docker_test_env_path);
        info!("  project_root: {:?}", config.project_root);
        info!("  log_dir: {:?}", log_dir);
        
        let container_config = ContainerConfig {
            docker_test_env_path: config.docker_test_env_path.clone().unwrap(),
            local_network_path: config.local_network_path.clone(),
            project_root: config.project_root.clone(),
            log_dir: log_dir.clone(),
            container_name: format!("local-network-test-{}", 
                config.session_name.as_deref().unwrap_or("default")),
            compose_project_name: format!("integration-test-{}", 
                uuid::Uuid::new_v4().to_string().split('-').next().unwrap()),
            startup_timeout: config.startup_timeout,
            auto_sync_images: config.auto_sync_images,
            ..ContainerConfig::default()
        };
        
        let container_manager = DindManager::new(container_config)?;
        
        Ok(Self {
            config,
            container_manager,
            _temp_dir,
            sync_result: None,
        })
    }
    
    /// Start the test harness and ensure the DinD container is running
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting local-network test harness");
        
        // Build images if requested
        if self.config.build_images {
            info!("Building local-network images");
            self.container_manager.build_host_images().await
                .context("Failed to build images")?;
        }
        
        // Start the DinD container
        self.container_manager.ensure_running().await
            .context("Failed to start DinD container")?;
        
        // Sync images if requested
        if self.config.auto_sync_images {
            info!("Syncing Docker images from host to DinD container");
            let sync_result = self.container_manager.sync_images().await
                .context("Failed to sync images")?;
            
            info!("Synced {} images, skipped {}, failed {}", 
                sync_result.total_synced, 
                sync_result.total_skipped,
                sync_result.total_failed
            );
            
            self.sync_result = Some(sync_result);
        }
        
        info!("Test harness started successfully");
        Ok(())
    }
    
    /// Execute a command in the test container
    pub async fn exec(&mut self, cmd: Vec<&str>, workdir: Option<&str>) -> Result<i32> {
        let exit_code = self.container_manager.exec_in_container(cmd, workdir).await?;
        Ok(exit_code as i32)
    }
    
    /// Start the local network using docker-compose
    pub async fn start_local_network(&mut self) -> Result<()> {
        info!("Starting local network with docker-compose");
        
        let compose_file = format!("{}/{}", LOCAL_NETWORK_MOUNT_PATH, DOCKER_COMPOSE_FILE);
        let exit_code = self.exec(
            vec!["docker-compose", "-f", &compose_file, "up", "-d", "--no-build"],
            Some(WORKSPACE_PATH)
        ).await?;
        
        if exit_code != 0 {
            return Err(anyhow::anyhow!("Failed to start local network"));
        }
        
        // Wait for services to be ready
        self.wait_for_network_ready().await?;
        
        Ok(())
    }
    
    /// Stop the local network
    pub async fn stop_local_network(&mut self) -> Result<()> {
        info!("Stopping local network");
        
        let compose_file = format!("{}/{}", LOCAL_NETWORK_MOUNT_PATH, DOCKER_COMPOSE_FILE);
        let exit_code = self.exec(
            vec!["docker-compose", "-f", &compose_file, "down", "-v"],
            Some(WORKSPACE_PATH)
        ).await?;
        
        if exit_code != 0 {
            warn!("Failed to cleanly stop local network");
        }
        
        Ok(())
    }
    
    /// Wait for the local network services to be ready
    async fn wait_for_network_ready(&mut self) -> Result<()> {
        info!("Waiting for local network services to be ready");
        
        // Check if graph-node is responding
        let max_attempts = 30;
        for i in 0..max_attempts {
            let exit_code = self.exec(
                vec![
                    "curl", "-s", "-f", 
                    "http://graph-node:8030/graphql",
                    "-H", "Content-Type: application/json",
                    "-d", r#"{"query":"{ indexingStatuses { subgraph } }"}"#
                ],
                None
            ).await?;
            
            if exit_code == 0 {
                info!("Graph node is ready");
                return Ok(());
            }
            
            if i < max_attempts - 1 {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
        
        Err(anyhow::anyhow!("Graph node failed to become ready"))
    }
    
    /// Get the session ID for this test run
    pub fn session_id(&self) -> &str {
        self.container_manager.session_id()
    }
    
    /// Get the log directory path
    pub fn log_dir(&self) -> PathBuf {
        self.config.log_dir.clone()
            .unwrap_or_else(|| self._temp_dir.as_ref()
                .map(|d| d.path().join("logs"))
                .unwrap_or_else(|| PathBuf::from("./logs")))
    }
    
    /// Get the image sync results
    pub fn sync_result(&self) -> Option<&crate::container::image_sync::ImageSyncResult> {
        self.sync_result.as_ref()
    }
    
    /// Print a summary of all log files created during this session
    pub fn print_log_summary(&self) {
        self.container_manager.print_log_summary();
    }
    
    /// Create a service inspector for real-time event monitoring
    pub fn create_service_inspector(&self) -> Result<ServiceInspector> {
        let docker = Docker::connect_with_local_defaults()
            .context("Failed to connect to Docker")?;
        
        let mut registry = ServiceEventRegistry::new();
        
        // Register built-in handlers for common services
        registry.register_handler(Box::new(PostgresEventHandler::new()));
        registry.register_handler(Box::new(GraphNodeEventHandler::new()));
        
        Ok(ServiceInspector::with_registry(docker, registry))
    }
    
    /// Get running container information for service inspection
    pub async fn get_running_containers(&self) -> Result<Vec<(String, String)>> {
        // In the DinD environment, we need to exec into the DinD container to get containers
        let output = self.container_manager.exec_simple(vec!["docker", "ps", "--format", "{{.Names}},{{.ID}}"]).await?;
        
        let mut containers = Vec::new();
        for line in output.lines() {
            if let Some((name, id)) = line.split_once(',') {
                // Remove leading slash from container name if present
                let clean_name = name.trim_start_matches('/');
                containers.push((clean_name.to_string(), id.to_string()));
            }
        }
        
        Ok(containers)
    }
    
    /// Start service inspection with default handlers
    pub async fn start_service_inspection(&self) -> Result<ServiceInspector> {
        let mut inspector = self.create_service_inspector()?;
        let containers = self.get_running_containers().await?;
        
        info!("Starting service inspection for {} containers", containers.len());
        inspector.start_streaming(containers).await?;
        
        Ok(inspector)
    }
}

/// Test context that provides utilities for writing tests
pub struct TestContext<'a> {
    harness: &'a mut LocalNetworkHarness,
}

impl<'a> TestContext<'a> {
    /// Create a new test context
    pub fn new(harness: &'a mut LocalNetworkHarness) -> Self {
        Self { harness }
    }
    
    /// Execute a command in the test environment
    pub async fn exec(&mut self, cmd: Vec<&str>) -> Result<i32> {
        self.harness.exec(cmd, None).await
    }
    
    /// Execute a command with a specific working directory
    pub async fn exec_in(&mut self, cmd: Vec<&str>, workdir: &str) -> Result<i32> {
        self.harness.exec(cmd, Some(workdir)).await
    }
    
    /// Deploy a subgraph to the local network
    pub async fn deploy_subgraph(&mut self, subgraph_path: &str, name: &str) -> Result<()> {
        info!("Deploying subgraph {} from {}", name, subgraph_path);
        
        // Create subgraph
        let exit_code = self.exec(vec![
            "npx", "graph", "create", "--node", "http://graph-node:8020", name
        ]).await?;
        
        if exit_code != 0 {
            return Err(anyhow::anyhow!("Failed to create subgraph"));
        }
        
        // Deploy subgraph
        let exit_code = self.exec_in(vec![
            "npx", "graph", "deploy", "--node", "http://graph-node:8020",
            "--ipfs", "http://ipfs:5001", name
        ], subgraph_path).await?;
        
        if exit_code != 0 {
            return Err(anyhow::anyhow!("Failed to deploy subgraph"));
        }
        
        Ok(())
    }
    
    /// Check if a container is running
    pub async fn container_running(&mut self, name: &str) -> Result<bool> {
        let exit_code = self.exec(vec![
            "docker", "ps", "-q", "-f", &format!("name={}", name)
        ]).await?;
        
        Ok(exit_code == 0)
    }
    
    /// Get logs from a container
    pub async fn container_logs(&mut self, name: &str, tail: Option<usize>) -> Result<()> {
        let mut cmd = vec!["docker", "logs"];
        
        let tail_str;
        if let Some(lines) = tail {
            cmd.push("--tail");
            tail_str = lines.to_string();
            cmd.push(&tail_str);
        }
        
        cmd.push(name);
        
        self.exec(cmd).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config() {
        let config = HarnessConfig::default();
        assert_eq!(config.startup_timeout, Duration::from_secs(60));
        assert!(config.auto_sync_images);
        assert!(!config.build_images);
    }
}