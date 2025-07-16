use std::path::PathBuf;
use std::time::Duration;

// Constants for default paths
const INTEGRATION_TESTS: &str = "integration-tests";
const LOGS: &str = "logs";
const CONTAINER_SESSIONS: &str = "container-sessions";

/// Configuration for the Docker-in-Docker container
#[derive(Debug, Clone)]
pub struct ContainerConfig {
    /// Path to the docker-test-env directory
    pub docker_test_env_path: PathBuf,
    /// Path to the local-network directory
    pub local_network_path: PathBuf,
    /// Project root directory (will be mounted as /workspace)
    pub project_root: PathBuf,
    /// Container name
    pub container_name: String,
    /// Docker compose project name
    pub compose_project_name: String,
    /// Timeout for waiting for Docker daemon to be ready
    pub docker_ready_timeout: Duration,
    /// Timeout for container startup
    pub startup_timeout: Duration,
    /// Whether to sync images from host by default
    pub auto_sync_images: bool,
    /// Log directory for container operations
    pub log_dir: PathBuf,
}

impl Default for ContainerConfig {
    fn default() -> Self {
        let current_dir = std::env::current_dir().expect("Failed to get current directory");
        let docker_test_env_path = current_dir.join(INTEGRATION_TESTS).join("docker-test-env");
        // Don't assume local-network path - let the caller specify it
        let local_network_path = current_dir.clone(); // Default to current dir, caller should override
        let project_root = current_dir.clone();
        let log_dir = current_dir.join(INTEGRATION_TESTS).join(LOGS).join(CONTAINER_SESSIONS);
        
        Self {
            docker_test_env_path,
            local_network_path,
            project_root,
            container_name: "integration-tests-dind".to_string(),
            compose_project_name: "integration-tests".to_string(),
            docker_ready_timeout: Duration::from_secs(60),
            startup_timeout: Duration::from_secs(30),
            auto_sync_images: false,
            log_dir,
        }
    }
}

impl ContainerConfig {
    /// Create a new container config with custom paths
    pub fn new(docker_test_env_path: PathBuf, local_network_path: PathBuf, project_root: PathBuf) -> Self {
        let log_dir = project_root.join("integration-tests/logs/container-sessions");
        Self {
            docker_test_env_path,
            local_network_path,
            project_root: project_root.clone(),
            log_dir,
            ..Default::default()
        }
    }
    
    /// Enable automatic image syncing
    pub fn with_auto_sync(mut self) -> Self {
        self.auto_sync_images = true;
        self
    }
    
    /// Set custom container name
    pub fn with_container_name(mut self, name: String) -> Self {
        self.container_name = name;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_default_config() {
        let config = ContainerConfig::default();
        
        assert_eq!(config.container_name, "integration-tests-dind");
        assert_eq!(config.compose_project_name, "integration-tests");
        assert_eq!(config.docker_ready_timeout, Duration::from_secs(60));
        assert_eq!(config.startup_timeout, Duration::from_secs(30));
        assert!(!config.auto_sync_images);
        assert!(config.log_dir.to_string_lossy().contains("integration-tests/logs/container-sessions"));
    }

    #[test]
    fn test_new_config() {
        let docker_path = PathBuf::from("/test/docker-env");
        let local_network_path = PathBuf::from("/test/local-network");
        let project_root = PathBuf::from("/test/project");
        let config = ContainerConfig::new(docker_path.clone(), local_network_path.clone(), project_root.clone());
        
        assert_eq!(config.docker_test_env_path, docker_path);
        assert_eq!(config.local_network_path, local_network_path);
        assert_eq!(config.project_root, project_root);
        assert_eq!(config.log_dir, project_root.join("integration-tests/logs/container-sessions"));
    }

    #[test]
    fn test_with_auto_sync() {
        let config = ContainerConfig::default().with_auto_sync();
        assert!(config.auto_sync_images);
    }

    #[test]
    fn test_with_container_name() {
        let config = ContainerConfig::default().with_container_name("custom-container".to_string());
        assert_eq!(config.container_name, "custom-container");
    }

    #[test]
    fn test_builder_pattern() {
        let config = ContainerConfig::default()
            .with_auto_sync()
            .with_container_name("test-container".to_string());
        
        assert!(config.auto_sync_images);
        assert_eq!(config.container_name, "test-container");
    }
}