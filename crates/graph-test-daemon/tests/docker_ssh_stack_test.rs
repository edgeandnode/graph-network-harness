//! Integration test that launches the Graph Protocol stack in Docker container via SSH
//!
//! This test uses graph-test-daemon to orchestrate services in a Docker container,
//! demonstrating the layered execution approach with SSH access.

use anyhow::{Context, Result};
use graph_test_daemon::GraphTestDaemon;
use harness_core::prelude::Daemon;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, warn};

const CONTAINER_NAME: &str = "graph-test-env";
const SSH_PORT: u16 = 2222;

/// Check if Docker is available
fn is_docker_available() -> bool {
    std::process::Command::new("docker")
        .arg("version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Ensure the test container is running
async fn ensure_container_running() -> Result<()> {
    // Check if container exists and is running
    let output = std::process::Command::new("docker")
        .args(["ps", "-q", "-f", &format!("name={}", CONTAINER_NAME)])
        .output()?;

    if !output.stdout.is_empty() {
        info!("Container {} is already running", CONTAINER_NAME);
        return Ok(());
    }

    // Get the setup script path
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")?;
    let setup_script = PathBuf::from(manifest_dir).join("tests/docker-test-env/setup.sh");

    info!("Running setup script: {:?}", setup_script);

    // Run the setup script
    let output = std::process::Command::new("bash")
        .arg(setup_script)
        .output()
        .context("Failed to run setup script")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Setup script failed: {}", stderr);
    }

    // Wait for SSH to be ready
    info!("Waiting for SSH to be ready...");
    smol::Timer::after(Duration::from_secs(5)).await;

    Ok(())
}

/// Stop and remove the container for cleanup
async fn cleanup_container() -> Result<()> {
    info!("Cleaning up container {}", CONTAINER_NAME);

    // Stop the container
    let _ = std::process::Command::new("docker")
        .args(["stop", CONTAINER_NAME])
        .output();

    // Remove the container
    let _ = std::process::Command::new("docker")
        .args(["rm", CONTAINER_NAME])
        .output();

    Ok(())
}

#[smol_potat::test]
async fn test_graph_stack_via_ssh_docker() -> Result<()> {
    // Initialize tracing for test
    let _ = tracing_subscriber::fmt::try_init();

    if !is_docker_available() {
        warn!("Skipping test - Docker not available");
        return Ok(());
    }

    // Ensure container is running
    ensure_container_running().await?;

    // Load the YAML configuration
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")?;
    let manifest_path = PathBuf::from(&manifest_dir);
    let config_path = manifest_path.join("configs/graph-stack-docker-test.yaml");
    
    // Change to the manifest directory so relative paths in the config work
    std::env::set_current_dir(&manifest_path)?;
    
    info!("Loading configuration from: {:?}", config_path);

    // Create the daemon with configuration
    let endpoint: SocketAddr = "127.0.0.1:9444".parse()?;
    let daemon = GraphTestDaemon::from_config(endpoint, &config_path)
        .await
        .context("Failed to create GraphTestDaemon")?;

    info!("GraphTestDaemon created successfully");

    // Start the daemon
    daemon.start().await.context("Failed to start daemon")?;

    info!("Daemon started successfully");

    // Launch the stack (currently just Anvil)
    info!("Launching Graph Protocol stack...");

    match daemon.launch_stack().await {
        Ok(_) => {
            info!("Stack launched successfully!");
            // The launch_stack() method already waits for services to become healthy
            // using the health check defined in the YAML configuration.
            // The health check runs curl via SSH in the container to verify Anvil is responding.
            info!("All services passed their health checks and are running!");
        }
        Err(e) => {
            warn!("Failed to launch stack: {}", e);
            return Err(e.into());
        }
    }

    // Stop the daemon
    info!("Stopping daemon...");
    daemon.stop().await.context("Failed to stop daemon")?;

    info!("Test completed successfully");

    // Note: We don't cleanup the container here to allow for debugging
    // In production tests, you might want to cleanup:
    // cleanup_container().await?;

    Ok(())
}

#[smol_potat::test]
#[ignore] // Run with --ignored flag to cleanup
async fn test_cleanup_docker_container() -> Result<()> {
    cleanup_container().await
}
