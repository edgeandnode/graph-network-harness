//! Integration test that launches the full Graph Protocol stack in Docker
//!
//! This test demonstrates launching multiple services (postgres, ipfs, anvil)
//! in a Docker container via SSH using the graph-test-daemon.

mod common;

use anyhow::{Context, Result};
use graph_test_daemon::GraphTestDaemon;
use harness_core::prelude::{BaseDaemon, Daemon};
use service_orchestration::StackConfig;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, warn};

#[smol_potat::test]
async fn test_full_graph_stack() -> Result<()> {
    // Initialize tracing for test
    let _ = tracing_subscriber::fmt::try_init();

    // Ensure the shared container is running
    common::shared_container::ensure_container_running()
        .await
        .context("Failed to ensure container is running")?;

    // Load the YAML configuration
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")?;
    let manifest_path = PathBuf::from(&manifest_dir);
    let config_path = manifest_path.join("configs/full-stack-docker-test.yaml");

    // Change to the manifest directory so relative paths in the config work
    std::env::set_current_dir(&manifest_path)?;

    info!("Loading configuration from: {:?}", config_path);

    // Load configuration from YAML file
    let config_content =
        std::fs::read_to_string(&config_path).context("Failed to read config file")?;

    let config: StackConfig =
        serde_yaml::from_str(&config_content).context("Failed to parse config YAML")?;

    // Create the daemon with configuration
    let endpoint: SocketAddr = "127.0.0.1:9445".parse()?;
    let mut builder = BaseDaemon::builder(config).with_endpoint(endpoint);

    // Register all services we want to test
    builder.wire_service::<graph_test_daemon::services::PostgresService>("postgres")?;
    builder.wire_service::<graph_test_daemon::services::IpfsService>("ipfs")?;
    builder.wire_service::<graph_test_daemon::services::AnvilService>("anvil")?;

    let daemon = GraphTestDaemon::from_builder(builder)
        .await
        .context("Failed to create GraphTestDaemon")?;

    info!("GraphTestDaemon created successfully");

    // Start the daemon
    daemon.start().await.context("Failed to start daemon")?;

    info!("Daemon started successfully");

    // Launch the stack
    info!("Launching Graph Protocol stack...");
    
    match daemon.launch_stack().await {
        Ok(_) => {
            info!("Stack launched successfully!");
            
            // Give services a moment to stabilize
            async_io::Timer::after(Duration::from_secs(2)).await;
            
            // Services should now be running
            info!("Verifying services are running...");
            
            // Let services run for a bit to ensure stability
            info!("Services launched, waiting for stability check...");
            async_io::Timer::after(Duration::from_secs(10)).await;
            
            info!("All services should be running!");
            
            // TODO: Add actual health checks once the API is available
            // For now, if launch_stack succeeded, we assume services are running
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

    Ok(())
}

#[smol_potat::test]
async fn test_postgres_service() -> Result<()> {
    // Initialize tracing for test
    let _ = tracing_subscriber::fmt::try_init();

    // Ensure the shared container is running
    common::shared_container::ensure_container_running()
        .await
        .context("Failed to ensure container is running")?;

    // Test just PostgreSQL service
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")?;
    let manifest_path = PathBuf::from(&manifest_dir);
    
    // Create a minimal config with just postgres
    let config_yaml = r#"
name: postgres-test
description: PostgreSQL service test

services:
  postgres:
    service_type: postgres
    target:
      type: layered
      params:
        binary: /usr/lib/postgresql/14/bin/postgres
        data_dir: /var/lib/postgresql/data
        host: 0.0.0.0
        port: 5432
      layers:
        - type: ssh
          host: localhost
          user: testuser
          port: 2222
          identity_file: tests/docker-test-env/ssh-keys/test_ed25519
          env:
            POSTGRES_USER: postgres
            POSTGRES_DB: graph-node
      command_template: "{binary} -D {data_dir} -h {host} -p {port}"
      health_check:
        command: pg_isready
        args:
          - "-h"
          - "localhost"
          - "-p"
          - "5432"
        interval: 2
        timeout: 1
        retries: 10
    dependencies: []
"#;

    std::env::set_current_dir(&manifest_path)?;

    let config: StackConfig = serde_yaml::from_str(config_yaml)?;

    let endpoint: SocketAddr = "127.0.0.1:9446".parse()?;
    let mut builder = BaseDaemon::builder(config).with_endpoint(endpoint);
    builder.wire_service::<graph_test_daemon::services::PostgresService>("postgres")?;

    let daemon = GraphTestDaemon::from_builder(builder).await?;
    daemon.start().await?;

    info!("Starting PostgreSQL service...");
    match daemon.launch_stack().await {
        Ok(_) => {
            info!("PostgreSQL started successfully");
            
            // Wait for service to stabilize
            async_io::Timer::after(Duration::from_secs(5)).await;
            
            info!("PostgreSQL should be running and accepting connections");
            
            // TODO: Add actual health check once the API is available
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to start PostgreSQL: {}", e));
        }
    }

    daemon.stop().await?;
    info!("PostgreSQL test completed successfully");
    Ok(())
}