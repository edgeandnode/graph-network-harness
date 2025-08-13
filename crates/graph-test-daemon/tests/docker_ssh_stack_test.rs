//! Integration test that launches the Graph Protocol stack in Docker container via SSH
//!
//! This test uses graph-test-daemon to orchestrate services in a Docker container,
//! demonstrating the layered execution approach with SSH access.

mod common;

use anyhow::{Context, Result};
use graph_test_daemon::GraphTestDaemon;
use harness_core::prelude::{BaseDaemon, Daemon};
use service_orchestration::StackConfig;
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing::{info, warn};

#[smol_potat::test]
async fn test_graph_stack_via_ssh_docker() -> Result<()> {
    // Initialize tracing for test
    let _ = tracing_subscriber::fmt::try_init();

    // Ensure the shared container is running
    common::shared_container::ensure_container_running()
        .await
        .context("Failed to ensure container is running")?;

    // Load the YAML configuration
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")?;
    let manifest_path = PathBuf::from(&manifest_dir);
    let config_path = manifest_path.join("configs/graph-stack-docker-test.yaml");

    // Change to the manifest directory so relative paths in the config work
    std::env::set_current_dir(&manifest_path)?;

    info!("Loading configuration from: {:?}", config_path);

    // Load configuration from YAML file
    let config_content =
        std::fs::read_to_string(&config_path).context("Failed to read config file")?;

    let config: StackConfig =
        serde_yaml::from_str(&config_content).context("Failed to parse config YAML")?;

    // Create the daemon with configuration
    // Since this test only has anvil service, use the builder to register only what we need
    let endpoint: SocketAddr = "127.0.0.1:9444".parse()?;
    let mut builder = BaseDaemon::builder(config).with_endpoint(endpoint);

    // Only register anvil service since that's all we have in the test config
    builder.wire_service::<graph_test_daemon::services::AnvilService>("anvil")?;

    let daemon = GraphTestDaemon::from_builder(builder)
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

    // Note: Container cleanup is handled by the shared_container module
    // It will be cleaned up when all tests complete or on exit/signal

    Ok(())
}

#[smol_potat::test]
async fn test_graph_stack_with_postgres() -> Result<()> {
    // Initialize tracing for test
    let _ = tracing_subscriber::fmt::try_init();

    // Ensure the shared container is running
    common::shared_container::ensure_container_running()
        .await
        .context("Failed to ensure container is running")?;

    // This test could use a different config that includes postgres
    // For now, just verify the container is accessible
    info!("Container is running and accessible for postgres test");

    Ok(())
}

#[smol_potat::test]
#[ignore] // Run with --ignored flag to manually cleanup
async fn test_cleanup_docker_container() -> Result<()> {
    common::shared_container::cleanup_test_container().await;
    Ok(())
}
