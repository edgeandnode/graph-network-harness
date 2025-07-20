//! Test harness that uses command-executor to orchestrate its own tests

use command_executor::{
    Executor, Target, Command, ProcessHandle,
    backends::local::LocalLauncher,
};
use std::path::{Path, PathBuf};
use std::time::Duration;
use anyhow::{Result, Context, bail};

/// Test harness that manages test infrastructure using command-executor itself
pub struct TestHarness {
    pub executor: Executor<LocalLauncher>,
    pub container_name: String,
    pub container_handle: Option<Box<dyn ProcessHandle>>,
    pub ssh_key_path: PathBuf,
    pub ssh_port: u16,
}

impl TestHarness {
    /// Set up the test environment
    pub async fn setup() -> Result<Self> {
        let executor = Executor::local("test-harness");
        let container_name = "command-executor-systemd-ssh-test".to_string();
        let ssh_port = 2223;
        
        // Determine paths
        let project_root = std::env::current_dir()
            .context("Failed to get current directory")?;
        let test_dir = project_root.join("tests/systemd-container");
        let ssh_keys_dir = test_dir.join("ssh-keys");
        let ssh_key_path = ssh_keys_dir.join("test_ed25519");
        
        // Create SSH keys directory
        std::fs::create_dir_all(&ssh_keys_dir)
            .context("Failed to create SSH keys directory")?;
        
        let mut harness = Self {
            executor,
            container_name,
            container_handle: None,
            ssh_key_path: ssh_key_path.clone(),
            ssh_port,
        };
        
        // Generate SSH keys if needed
        if !ssh_key_path.exists() {
            println!("Generating SSH keys...");
            harness.generate_ssh_keys(&ssh_keys_dir).await?;
        }
        
        // Build and start container
        println!("Starting systemd container...");
        harness.start_container(&test_dir).await?;
        
        // Wait for container to be ready
        println!("Waiting for container to be ready...");
        harness.wait_for_container_ready().await?;
        
        Ok(harness)
    }
    
    /// Generate SSH keys for test authentication
    async fn generate_ssh_keys(&self, ssh_keys_dir: &Path) -> Result<()> {
        let key_path = ssh_keys_dir.join("test_ed25519");
        
        // Generate ED25519 key
        let keygen_cmd = Command::builder("ssh-keygen")
            .arg("-t").arg("ed25519")
            .arg("-f").arg(key_path.to_str().unwrap())
            .arg("-N").arg("")
            .arg("-C").arg("test@command-executor")
            .build();
        
        let result = self.executor.execute(&Target::Command, keygen_cmd).await
            .context("Failed to generate SSH key")?;
        
        if !result.success() {
            bail!("ssh-keygen failed with status: {:?}", result.status);
        }
        
        // Create authorized_keys file
        let pub_key_path = ssh_keys_dir.join("test_ed25519.pub");
        let authorized_keys_path = ssh_keys_dir.join("authorized_keys");
        
        let cp_cmd = Command::builder("cp")
            .arg(pub_key_path.to_str().unwrap())
            .arg(authorized_keys_path.to_str().unwrap())
            .build();
        
        let _ = self.executor.execute(&Target::Command, cp_cmd).await
            .context("Failed to create authorized_keys")?;
        
        Ok(())
    }
    
    /// Build and start the systemd container
    async fn start_container(&mut self, test_dir: &Path) -> Result<()> {
        // First, stop any existing container
        let stop_cmd = Command::builder("docker")
            .arg("rm").arg("-f").arg(&self.container_name)
            .build();
        
        // Ignore errors - container might not exist
        let _ = self.executor.execute(&Target::Command, stop_cmd).await;
        
        // Build the Docker image
        let build_cmd = Command::builder("docker-compose")
            .arg("-f").arg(test_dir.join("docker-compose.yaml").to_str().unwrap())
            .arg("build")
            .current_dir(test_dir)
            .build();
        
        let result = self.executor.execute(&Target::Command, build_cmd).await
            .context("Failed to build Docker image")?;
        
        if !result.success() {
            bail!("Docker build failed with status: {:?}", result.status);
        }
        
        // Start the container
        let up_cmd = Command::builder("docker-compose")
            .arg("-f").arg(test_dir.join("docker-compose.yaml").to_str().unwrap())
            .arg("up").arg("-d")
            .current_dir(test_dir)
            .build();
        
        let result = self.executor.execute(&Target::Command, up_cmd).await
            .context("Failed to start container")?;
        
        if !result.success() {
            bail!("Docker compose up failed with status: {:?}", result.status);
        }
        
        Ok(())
    }
    
    /// Wait for the container to be ready (systemd running and SSH accessible)
    async fn wait_for_container_ready(&self) -> Result<()> {
        let max_attempts = 30;
        let mut attempts = 0;
        
        // Wait for systemd to be running
        println!("Waiting for systemd to initialize...");
        loop {
            attempts += 1;
            if attempts > max_attempts {
                bail!("Timeout waiting for systemd to be ready");
            }
            
            let check_cmd = Command::builder("docker")
                .arg("exec").arg(&self.container_name)
                .arg("bash").arg("-c")
                .arg("systemctl is-system-running 2>&1 || echo $?")
                .build();
            
            if let Ok(result) = self.executor.execute(&Target::Command, check_cmd).await {
                if result.success() || result.output.contains("degraded") {
                    println!("Systemd is ready");
                    break;
                }
            }
            
            println!("Waiting for systemd... ({}/{})", attempts, max_attempts);
            smol::Timer::after(Duration::from_secs(1)).await;
        }
        
        // Wait for SSH to be accessible
        println!("Waiting for SSH to be ready...");
        attempts = 0;
        loop {
            attempts += 1;
            if attempts > max_attempts {
                bail!("Timeout waiting for SSH to be ready");
            }
            
            let nc_cmd = Command::builder("nc")
                .arg("-z").arg("localhost").arg(self.ssh_port.to_string())
                .build();
            
            if let Ok(result) = self.executor.execute(&Target::Command, nc_cmd).await {
                if result.success() {
                    println!("SSH is ready on port {}", self.ssh_port);
                    break;
                }
            }
            
            println!("Waiting for SSH... ({}/{})", attempts, max_attempts);
            smol::Timer::after(Duration::from_secs(1)).await;
        }
        
        Ok(())
    }
    
    /// Get SSH configuration for connecting to the test container
    #[cfg(feature = "ssh")]
    pub fn ssh_config(&self) -> command_executor::backends::ssh::SshConfig {
        command_executor::backends::ssh::SshConfig::new("localhost")
            .with_user("testuser")
            .with_port(self.ssh_port)
            .with_identity_file(&self.ssh_key_path)
            .with_extra_arg("-o")
            .with_extra_arg("StrictHostKeyChecking=no")
            .with_extra_arg("-o")
            .with_extra_arg("UserKnownHostsFile=/dev/null")
    }
    
    /// Check if the container is running
    pub async fn is_running(&self) -> bool {
        let ps_cmd = Command::builder("docker")
            .arg("ps").arg("-q").arg("-f")
            .arg(format!("name={}", self.container_name))
            .build();
        
        if let Ok(result) = self.executor.execute(&Target::Command, ps_cmd).await {
            result.success()
        } else {
            false
        }
    }
    
    /// Get container logs for debugging
    pub async fn get_logs(&self) -> Result<String> {
        // For now, we'll return a placeholder since we need to use launch() to capture output
        Ok("Container logs would be captured here".to_string())
    }
    
    /// Clean up the test environment
    pub async fn teardown(self) -> Result<()> {
        println!("Cleaning up test environment...");
        
        // Stop the container
        let down_cmd = Command::builder("docker")
            .arg("rm").arg("-f").arg(&self.container_name)
            .build();
        
        let _ = self.executor.execute(&Target::Command, down_cmd).await
            .context("Failed to stop container")?;
        
        Ok(())
    }
}

/// Test guard that ensures cleanup even on panic
pub struct TestGuard {
    harness: Option<TestHarness>,
}

impl TestGuard {
    pub async fn setup() -> Result<Self> {
        let harness = TestHarness::setup().await?;
        Ok(Self { harness: Some(harness) })
    }
    
    pub fn harness(&self) -> &TestHarness {
        self.harness.as_ref().expect("TestGuard already dropped")
    }
}

impl Drop for TestGuard {
    fn drop(&mut self) {
        if let Some(harness) = self.harness.take() {
            // Schedule cleanup on smol runtime
            std::thread::spawn(move || {
                smol::block_on(async move {
                    let _ = harness.teardown().await;
                });
            });
        }
    }
}