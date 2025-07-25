//! Graph Test Daemon Binary
//!
//! A specialized daemon for Graph Protocol integration testing that provides
//! domain-specific actions and service types for automated test workflows.

use clap::{Arg, Command};
use graph_test_daemon::{GraphTestDaemon, Daemon};
use std::net::SocketAddr;
use tracing::{info, error};
use tracing_subscriber;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    smol::block_on(async_main())
}

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    let matches = Command::new("graph-test-daemon")
        .version("0.1.0")
        .about("Graph Protocol specialized testing daemon")
        .arg(
            Arg::new("endpoint")
                .long("endpoint")
                .short('e')
                .value_name("ADDRESS")
                .help("WebSocket endpoint to bind to")
                .default_value("127.0.0.1:9443"),
        )
        .arg(
            Arg::new("registry-path")
                .long("registry-path")
                .short('r')
                .value_name("PATH")
                .help("Path for persistent service registry")
                .default_value("./graph-test-registry.db"),
        )
        .get_matches();
    
    let endpoint: SocketAddr = matches
        .get_one::<String>("endpoint")
        .unwrap()
        .parse()
        .expect("Invalid endpoint address");
    
    info!("Starting Graph Test Daemon on {}", endpoint);
    
    // Create and start the daemon
    let daemon = GraphTestDaemon::new(endpoint).await?;
    
    info!("Graph Test Daemon started successfully");
    info!("Available service types: anvil-blockchain, ipfs-node, postgres-db, graph-node");
    info!("Available actions: deploy-subgraph, start-indexing, mine-blocks, create-allocation, trigger-reorg, setup-stack, query-subgraph, wait-for-sync");
    
    // Start the daemon
    if let Err(e) = daemon.start().await {
        error!("Failed to start daemon: {}", e);
        return Err(e.into());
    }
    
    // Keep the daemon running
    // In a real implementation, this would handle signals and graceful shutdown
    loop {
        smol::Timer::after(std::time::Duration::from_secs(1)).await;
    }
}