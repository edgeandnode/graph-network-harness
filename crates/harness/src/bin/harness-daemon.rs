//! Harness daemon with WebSocket server
//!
//! This is the daemon that runs the WebSocket server and manages services.

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "harness-daemon")]
#[command(about = "Harness daemon for managing services", long_about = None)]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value = "9001")]
    port: u16,

    /// Data directory for daemon state
    #[arg(short, long, default_value = "./harness-daemon-data")]
    data_dir: PathBuf,
}

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt().with_target(false).init();

    let args = Args::parse();

    info!("Starting harness daemon on port {}", args.port);
    info!("Data directory: {}", args.data_dir.display());

    // Create data directory if it doesn't exist
    std::fs::create_dir_all(&args.data_dir)?;

    // Run with smol
    smol::block_on(async { harness::daemon::run(&args.data_dir, args.port).await })
}
