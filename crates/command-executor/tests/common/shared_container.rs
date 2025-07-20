//! Shared container management for tests
//! 
//! This module provides a shared container that is started once before all tests
//! and cleaned up after all tests complete.

use std::sync::{Arc, Mutex, OnceLock};
use anyhow::{Result, Context};
use command_executor::{Executor, Target, Command};

// Store the container name globally so we can clean it up
static CONTAINER_NAME: &str = "command-executor-systemd-ssh-test";

// Global container guard that will clean up on drop
static CONTAINER_GUARD: OnceLock<ContainerCleanupGuard> = OnceLock::new();

struct ContainerCleanupGuard {
    container_name: String,
}

impl Drop for ContainerCleanupGuard {
    fn drop(&mut self) {
        eprintln!("Cleaning up test container: {}", self.container_name);
        // We need to do synchronous cleanup in drop
        std::process::Command::new("docker")
            .args(&["rm", "-f", &self.container_name])
            .output()
            .ok();
    }
}

/// Setup function that ensures the container is running
/// This can be called by multiple tests safely - it will only start the container once
pub async fn ensure_container_running() -> Result<()> {
    // Check if container is already running
    let check_cmd = Command::builder("docker")
        .arg("ps")
        .arg("-q")
        .arg("-f")
        .arg(format!("name={}", CONTAINER_NAME))
        .build();
    
    let executor = Executor::local("container-check");
    let result = executor.execute(&Target::Command, check_cmd).await?;
    
    if !result.output.trim().is_empty() {
        // Container is already running
        return Ok(());
    }
    
    eprintln!("Starting shared test container...");
    
    // Container not running, start it
    let project_root = std::env::current_dir()
        .context("Failed to get current directory")?;
    let test_dir = project_root.join("tests/systemd-container");
    
    // Build the Docker image
    let build_cmd = Command::builder("docker-compose")
        .arg("-f")
        .arg(test_dir.join("docker-compose.yaml").to_str().unwrap())
        .arg("build")
        .current_dir(&test_dir)
        .build();
    
    let result = executor.execute(&Target::Command, build_cmd).await
        .context("Failed to build Docker image")?;
    
    if !result.success() {
        anyhow::bail!("Docker build failed: {}", result.output);
    }
    
    // Start the container
    let up_cmd = Command::builder("docker-compose")
        .arg("-f")
        .arg(test_dir.join("docker-compose.yaml").to_str().unwrap())
        .arg("up")
        .arg("-d")
        .current_dir(&test_dir)
        .build();
    
    let result = executor.execute(&Target::Command, up_cmd).await
        .context("Failed to start container")?;
    
    if !result.success() {
        anyhow::bail!("Docker compose up failed: {}", result.output);
    }
    
    // Wait for container to be ready
    wait_for_container_ready().await?;
    
    // Register cleanup guard
    CONTAINER_GUARD.get_or_init(|| ContainerCleanupGuard {
        container_name: CONTAINER_NAME.to_string(),
    });
    
    eprintln!("Shared test container is ready!");
    Ok(())
}

async fn wait_for_container_ready() -> Result<()> {
    use std::time::Duration;
    
    let executor = Executor::local("container-wait");
    let max_attempts = 30;
    
    // Wait for systemd
    eprintln!("Waiting for systemd to initialize...");
    for i in 1..=max_attempts {
        let check_cmd = Command::builder("docker")
            .arg("exec")
            .arg(CONTAINER_NAME)
            .arg("bash")
            .arg("-c")
            .arg("systemctl is-system-running 2>&1 || echo $?")
            .build();
        
        if let Ok(result) = executor.execute(&Target::Command, check_cmd).await {
            if result.success() || result.output.contains("degraded") {
                eprintln!("Systemd is ready");
                break;
            }
        }
        
        if i == max_attempts {
            anyhow::bail!("Timeout waiting for systemd");
        }
        
        eprintln!("Waiting for systemd... ({}/{})", i, max_attempts);
        smol::Timer::after(Duration::from_secs(1)).await;
    }
    
    // Wait for SSH
    eprintln!("Waiting for SSH to be ready...");
    for i in 1..=max_attempts {
        let nc_cmd = Command::builder("nc")
            .arg("-z")
            .arg("localhost")
            .arg("2223")
            .build();
        
        if let Ok(result) = executor.execute(&Target::Command, nc_cmd).await {
            if result.success() {
                eprintln!("SSH is ready on port 2223");
                return Ok(());
            }
        }
        
        if i == max_attempts {
            anyhow::bail!("Timeout waiting for SSH");
        }
        
        eprintln!("Waiting for SSH... ({}/{})", i, max_attempts);
        smol::Timer::after(Duration::from_secs(1)).await;
    }
    
    Ok(())
}

/// Get SSH configuration for the shared container
#[cfg(feature = "ssh")]
pub fn get_ssh_config() -> command_executor::backends::ssh::SshConfig {
    let ssh_key_path = std::env::current_dir()
        .unwrap()
        .join("tests/systemd-container/ssh-keys/test_ed25519");
    
    command_executor::backends::ssh::SshConfig::new("localhost")
        .with_user("testuser")
        .with_port(2223)
        .with_identity_file(&ssh_key_path)
        .with_extra_arg("-o")
        .with_extra_arg("StrictHostKeyChecking=no")
        .with_extra_arg("-o")
        .with_extra_arg("UserKnownHostsFile=/dev/null")
}

/// Helper macro to setup shared container for a test
#[macro_export]
macro_rules! with_shared_container {
    ($test_body:expr) => {
        match $crate::common::shared_container::ensure_container_running().await {
            Ok(()) => {
                $test_body
            }
            Err(e) => {
                eprintln!("Failed to ensure container is running: {}", e);
                panic!("Container setup failed");
            }
        }
    };
}