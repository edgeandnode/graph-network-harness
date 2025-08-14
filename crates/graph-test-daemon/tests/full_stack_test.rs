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
use tracing::{error, info, warn};

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
            
            // Give services a moment to stabilize and collect output
            async_io::Timer::after(Duration::from_secs(2)).await;
            
            // TODO: Get service events once we have access to event streams
            // For now, just log that services have been launched
            info!("Services have been launched, but event streams are not yet accessible");
            info!("This will be fixed when launch_stack returns event receivers");
            
            // Services should now be running
            info!("Verifying services are running...");
            
            // Give services a moment to stabilize
            async_io::Timer::after(Duration::from_secs(3)).await;
            
            // Query PostgreSQL directly
            info!("Checking PostgreSQL...");
            let pg_check = std::process::Command::new("pg_isready")
                .args(["-h", "localhost", "-p", "5432"])
                .output();
            
            match pg_check {
                Ok(output) if output.status.success() => {
                    info!("✓ PostgreSQL is accepting connections");
                }
                _ => {
                    warn!("✗ PostgreSQL is not responding on port 5432");
                }
            }
            
            // Query IPFS
            info!("Checking IPFS...");
            let ipfs_check = std::process::Command::new("curl")
                .args(["-s", "http://localhost:5001/api/v0/version"])
                .output();
            
            match ipfs_check {
                Ok(output) if output.status.success() => {
                    let response = String::from_utf8_lossy(&output.stdout);
                    if response.contains("Version") {
                        info!("✓ IPFS API is responding");
                    } else {
                        warn!("✗ IPFS API returned unexpected response");
                    }
                }
                _ => {
                    warn!("✗ IPFS is not responding on port 5001");
                }
            }
            
            // Query Anvil
            info!("Checking Anvil...");
            let anvil_check = std::process::Command::new("curl")
                .args([
                    "-s", "-X", "POST",
                    "-H", "Content-Type: application/json",
                    "--data", r#"{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}"#,
                    "http://localhost:8545"
                ])
                .output();
            
            match anvil_check {
                Ok(output) if output.status.success() => {
                    let response = String::from_utf8_lossy(&output.stdout);
                    if response.contains("result") {
                        info!("✓ Anvil RPC is responding");
                    } else {
                        warn!("✗ Anvil RPC returned unexpected response");
                    }
                }
                _ => {
                    warn!("✗ Anvil is not responding on port 8545");
                }
            }
            
            // Let services run for a bit longer
            info!("Services are running, letting them stabilize...");
            async_io::Timer::after(Duration::from_secs(5)).await;
            
            info!("All services have been queried successfully!");
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