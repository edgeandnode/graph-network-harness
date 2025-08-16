//! Full Graph Protocol stack demonstration
//! 
//! This binary launches the full Graph Protocol stack in Docker for testing and development.
//! It requires the GRAPH_NODE_SRC environment variable to be set to build graph-node.

use anyhow::{Context, Result};
use graph_test_daemon::GraphTestDaemon;
use harness_core::prelude::{BaseDaemon, Daemon, DeploymentTask};
use harness_core::task::YamlTask;
use service_orchestration::StackConfig;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, warn};

#[smol_potat::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Try to load .env file if it exists
    if let Ok(path) = dotenvy::dotenv() {
        info!("Loaded environment from: {:?}", path);
    }

    // Check for GRAPH_NODE_SRC environment variable
    let _graph_node_src = match std::env::var("GRAPH_NODE_SRC") {
        Ok(path) => {
            let path = PathBuf::from(path);
            if !path.exists() {
                anyhow::bail!("GRAPH_NODE_SRC path does not exist: {:?}", path);
            }
            if !path.join("Cargo.toml").exists() {
                anyhow::bail!("GRAPH_NODE_SRC does not appear to be a Cargo project: {:?}", path);
            }
            info!("Using graph-node source at: {:?}", path);
            path
        }
        Err(_) => {
            eprintln!("Error: GRAPH_NODE_SRC environment variable not set");
            eprintln!("To run this demo, set GRAPH_NODE_SRC to the path of your graph-node repository");
            eprintln!("You can either:");
            eprintln!("  1. Set it in your environment: export GRAPH_NODE_SRC=/path/to/graph-node");
            eprintln!("  2. Create a .env file with: GRAPH_NODE_SRC=/path/to/graph-node");
            eprintln!("  3. Run with: GRAPH_NODE_SRC=/path/to/graph-node cargo run --bin full_stack_demo");
            std::process::exit(1);
        }
    };

    // Ensure the shared container is running
    ensure_container_running()
        .await
        .context("Failed to ensure container is running")?;

    // Load the YAML configuration
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let manifest_path = PathBuf::from(manifest_dir);
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
    let builder = BaseDaemon::builder(config).with_endpoint(endpoint);

    // GraphTestDaemon::from_builder will auto-wire all known services and tasks
    let daemon = GraphTestDaemon::from_builder(builder)
        .await
        .context("Failed to create GraphTestDaemon")?;

    info!("GraphTestDaemon created successfully");

    // Start the daemon
    daemon.start().await.context("Failed to start daemon")?;

    info!("Daemon started successfully");

    // Check build-graph-node task before launching stack
    info!("Checking build-graph-node task...");

    if let Some(build_task) = daemon.base.get_task::<YamlTask>("build-graph-node") {
        // Check if it needs to run
        match build_task.validate().await {
            Ok(true) => info!("  Graph node binary already built"),
            Ok(false) => info!("  Graph node needs to be built"),
            Err(e) => info!("  Could not validate build status: {}", e),
        }
    } else {
        info!("  build-graph-node task not found");
    }

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
                    "-s",
                    "-X",
                    "POST",
                    "-H",
                    "Content-Type: application/json",
                    "--data",
                    r#"{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}"#,
                    "http://localhost:8545",
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
            info!("");
            info!("Stack is running. Press Ctrl+C to stop and clean up.");
            
            // Wait for Ctrl+C
            let (tx, rx) = smol::channel::bounded(1);
            ctrlc::set_handler(move || {
                let _ = smol::block_on(tx.send(()));
            }).expect("Error setting Ctrl-C handler");
            
            let _ = smol::block_on(rx.recv());
            
            info!("\nShutting down...");
        }
        Err(e) => {
            warn!("Failed to launch stack: {}", e);
            return Err(e.into());
        }
    }

    // Stop the daemon
    info!("Stopping daemon...");
    daemon.stop().await.context("Failed to stop daemon")?;

    info!("Demo completed successfully");

    Ok(())
}

async fn ensure_container_running() -> Result<()> {
    use std::process::Command;

    let container_name = "harness-test-runner";
    
    // Check if container exists and is running
    let status = Command::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", container_name])
        .output()
        .context("Failed to check container status")?;

    if !status.status.success() || !String::from_utf8_lossy(&status.stdout).trim().eq("true") {
        info!("Container {} is not running, starting it...", container_name);
        
        // Try to remove existing container first
        let _ = Command::new("docker")
            .args(["rm", "-f", container_name])
            .output();

        // Build the test environment if needed
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let docker_test_env = PathBuf::from(manifest_dir).join("docker-test-env");
        
        info!("Building test environment...");
        let build_status = Command::new("docker-compose")
            .args(["build"])
            .current_dir(&docker_test_env)
            .status()
            .context("Failed to build test environment")?;

        if !build_status.success() {
            anyhow::bail!("Failed to build test environment");
        }

        // Start the container
        info!("Starting test container...");
        let up_status = Command::new("docker-compose")
            .args(["up", "-d"])
            .current_dir(&docker_test_env)
            .status()
            .context("Failed to start test container")?;

        if !up_status.success() {
            anyhow::bail!("Failed to start test container");
        }

        // Wait for SSH to be ready
        info!("Waiting for SSH to be ready...");
        for _ in 0..30 {
            let ssh_check = Command::new("ssh")
                .args([
                    "-o", "StrictHostKeyChecking=no",
                    "-o", "ConnectTimeout=1",
                    "-i", "tests/docker-test-env/ssh-keys/test_ed25519",
                    "-p", "2222",
                    "testuser@localhost",
                    "echo", "ready"
                ])
                .output();

            if let Ok(output) = ssh_check {
                if output.status.success() {
                    info!("SSH is ready");
                    break;
                }
            }
            
            std::thread::sleep(Duration::from_secs(1));
        }
    } else {
        info!("Container {} is already running", container_name);
    }

    Ok(())
}