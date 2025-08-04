//! Graph Test Daemon Binary - New Architecture
//!
//! A specialized daemon for Graph Protocol integration testing that provides
//! domain-specific services with actions for automated test workflows.

use clap::{Arg, Command};
use graph_test_daemon::GraphTestDaemon;
use harness_core::prelude::Daemon;
use std::net::SocketAddr;
use tracing::{error, info};
use tracing_subscriber;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    smol::block_on(async_main())
}

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    let matches = Command::new("graph-test-daemon")
        .version("0.1.0")
        .about("Graph Protocol specialized testing daemon with actionable services")
        .arg(
            Arg::new("endpoint")
                .long("endpoint")
                .short('e')
                .value_name("ADDRESS")
                .help("WebSocket endpoint to bind to")
                .default_value("127.0.0.1:9443"),
        )
        .arg(
            Arg::new("config")
                .long("config")
                .short('c')
                .value_name("PATH")
                .help("Path to YAML configuration file")
                .required(true),
        )
        .get_matches();

    let endpoint: SocketAddr = matches
        .get_one::<String>("endpoint")
        .unwrap()
        .parse()
        .expect("Invalid endpoint address");

    info!("Starting Graph Test Daemon on {}", endpoint);

    // Create and start the daemon
    let config_path = matches.get_one::<String>("config").unwrap();
    info!("Loading daemon configuration from: {}", config_path);
    let daemon = GraphTestDaemon::from_config(endpoint, config_path).await?;

    info!("Graph Test Daemon created successfully");

    // Start the daemon
    if let Err(e) = daemon.start().await {
        error!("Failed to start daemon: {}", e);
        return Err(e.into());
    }

    info!("Graph Test Daemon is running");

    // Keep the daemon running
    // In a real implementation, this would handle signals and graceful shutdown
    loop {
        smol::Timer::after(std::time::Duration::from_secs(1)).await;
    }
}
