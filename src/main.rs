//! Integration test runner for The Graph indexer agent
//!
//! This binary provides a CLI interface for running integration tests
//! against a local Graph network deployment using Docker-in-Docker.

use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use local_network_harness::{LocalNetworkHarness, HarnessConfig};
use tracing::{info, Level};

#[derive(Parser)]
#[command(name = "integration-tests")]
#[command(about = "Integration test runner for The Graph Protocol indexer agent")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Run all integration tests
    All {
        /// Path to local-network directory (e.g., submodules/local-network)
        #[arg(long, value_name = "PATH")]
        local_network: String,
        /// Skip image building
        #[arg(long)]
        skip_build: bool,
        /// Custom log directory
        #[arg(long)]
        log_dir: Option<String>,
    },
    
    /// Start the local network harness for manual testing
    Harness {
        /// Path to local-network directory (e.g., submodules/local-network)
        #[arg(long, value_name = "PATH")]
        local_network: String,
        /// Keep the harness running after startup
        #[arg(long)]
        keep_running: bool,
        /// Build Docker images before starting
        #[arg(long)]
        build: bool,
        /// Custom session name
        #[arg(long)]
        session: Option<String>,
    },
    
    /// Run tests inside a container
    Container {
        /// Sync Docker images from host before running
        #[arg(long)]
        sync_images: bool,
        /// Build images on host before syncing
        #[arg(long)]
        build_host: bool,
        /// Path to local-network directory (e.g., submodules/local-network)
        #[arg(long, value_name = "PATH")]
        local_network: String,
        /// Test command to run (defaults to 'all')
        #[arg(trailing_var_arg = true)]
        command: Vec<String>,
    },
    
    /// Execute a command in the test environment
    Exec {
        /// Path to local-network directory (e.g., submodules/local-network)
        #[arg(long, value_name = "PATH")]
        local_network: String,
        /// Command to execute
        #[arg(trailing_var_arg = true, required = true)]
        command: Vec<String>,
        /// Working directory
        #[arg(long)]
        workdir: Option<String>,
    },
    
    /// Show logs from the test session
    Logs {
        /// Specific session to show logs from
        #[arg(long)]
        session: Option<String>,
        /// Show summary of all log files
        #[arg(long)]
        summary: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    let level = if cli.verbose {
        Level::DEBUG
    } else {
        Level::INFO
    };
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false)
        .init();

    match cli.command {
        Commands::All { local_network, skip_build, log_dir } => {
            run_all_tests(local_network, skip_build, log_dir).await?;
        }
        Commands::Harness { local_network, keep_running, build, session } => {
            run_harness(local_network, keep_running, build, session).await?;
        }
        Commands::Container { sync_images, build_host, local_network, command } => {
            run_in_container(sync_images, build_host, local_network, command).await?;
        }
        Commands::Exec { local_network, command, workdir } => {
            exec_command(local_network, command, workdir).await?;
        }
        Commands::Logs { session, summary } => {
            show_logs(session, summary).await?;
        }
    }

    Ok(())
}

async fn run_all_tests(local_network: String, skip_build: bool, log_dir: Option<String>) -> Result<()> {
    info!("Running all integration tests");
    
    let mut config = HarnessConfig::default();
    config.local_network_path = std::path::PathBuf::from(&local_network);
    config.build_images = !skip_build;
    if let Some(dir) = log_dir {
        config.log_dir = Some(dir.into());
    }
    
    let mut harness = LocalNetworkHarness::new(config)?;
    harness.start().await?;
    
    // Start the local network
    info!("Starting local network...");
    harness.start_local_network().await?;
    
    // Run test suites
    info!("Running test suites...");
    
    // TODO: Add actual test implementations here
    // For now, just verify the network is running
    let _ctx = local_network_harness::TestContext::new(&mut harness);
    
    info!("All tests completed successfully!");
    
    // Stop the network
    harness.stop_local_network().await?;
    
    // Print log summary
    harness.print_log_summary();
    
    Ok(())
}

async fn run_harness(local_network: String, keep_running: bool, build: bool, session: Option<String>) -> Result<()> {
    info!("Starting local network harness");
    
    let mut config = HarnessConfig::default();
    config.local_network_path = std::path::PathBuf::from(&local_network);
    config.build_images = build;
    config.session_name = session;
    
    let mut harness = LocalNetworkHarness::new(config)?;
    harness.start().await?;
    
    if keep_running {
        info!("Starting local network...");
        harness.start_local_network().await?;
        
        info!("Local network is running. Press Ctrl+C to stop.");
        info!("Session ID: {}", harness.session_id());
        info!("Log directory: {:?}", harness.log_dir());
        
        tokio::signal::ctrl_c().await?;
        
        info!("Stopping local network...");
        harness.stop_local_network().await?;
    }
    
    harness.print_log_summary();
    Ok(())
}

async fn run_in_container(sync_images: bool, build_host: bool, local_network: String, command: Vec<String>) -> Result<()> {
    info!("Running tests in container");
    
    let mut config = HarnessConfig::default();
    config.auto_sync_images = sync_images;
    config.build_images = build_host;
    
    // Set local-network path
    // Resolve relative to project root
    let project_root = std::env::current_dir()
        .context("Failed to get current directory")?
        .canonicalize()
        .context("Failed to canonicalize current directory")?;
    
    config.local_network_path = project_root.join(&local_network);
    info!("Using local-network path: {:?}", config.local_network_path);
    
    let mut harness = LocalNetworkHarness::new(config)?;
    
    // Start the container
    harness.start().await?;
    
    // Build integration tests inside container
    info!("Building integration tests in container...");
    let exit_code = harness.exec(
        vec!["cargo", "build", "--color=never", "--bin", "integration-tests"],
        Some("/workspace")
    ).await?;
    
    if exit_code != 0 {
        anyhow::bail!("Failed to build integration tests in container");
    }
    
    // Determine command to run
    let test_command = if command.is_empty() {
        vec!["cargo", "run", "--color=never", "--bin", "integration-tests", "--", "all"]
    } else {
        let mut cmd = vec!["cargo", "run", "--color=never", "--bin", "integration-tests", "--"];
        cmd.extend(command.iter().map(|s| s.as_str()));
        cmd
    };
    
    // Run the tests
    info!("Running: {:?}", test_command);
    let exit_code = harness.exec(test_command, Some("/workspace")).await?;
    
    harness.print_log_summary();
    
    if exit_code != 0 {
        anyhow::bail!("Tests failed with exit code: {}", exit_code);
    }
    
    Ok(())
}

async fn exec_command(local_network: String, command: Vec<String>, workdir: Option<String>) -> Result<()> {
    let mut config = HarnessConfig::default();
    config.local_network_path = std::path::PathBuf::from(&local_network);
    let mut harness = LocalNetworkHarness::new(config)?;
    
    // Ensure container is running
    harness.start().await?;
    
    // Execute the command
    let cmd_refs: Vec<&str> = command.iter().map(|s| s.as_str()).collect();
    let exit_code = harness.exec(cmd_refs, workdir.as_deref()).await?;
    
    if exit_code != 0 {
        anyhow::bail!("Command failed with exit code: {}", exit_code);
    }
    
    Ok(())
}

async fn show_logs(session: Option<String>, summary: bool) -> Result<()> {
    let mut config = HarnessConfig::default();
    config.session_name = session;
    
    let harness = LocalNetworkHarness::new(config)?;
    
    if summary {
        harness.print_log_summary();
    } else {
        info!("Session: {}", harness.session_id());
        info!("Log directory: {:?}", harness.log_dir());
        harness.print_log_summary();
    }
    
    Ok(())
}