//! Demo daemon with WebSocket server
//!
//! Run with: cargo run --example demo-daemon-ws --features smol

use anyhow::Result;
use std::path::PathBuf;
use tracing::info;

fn main() -> Result<()> {
    // Use smol runtime
    smol::block_on(async {
        run_daemon().await
    })
}

async fn run_daemon() -> Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .init();

    info!("🚀 Starting Demo Daemon with WebSocket Server");
    info!("📦 This simulates a simple microservices platform");

    // Use the default harness data directory
    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("harness");
    std::fs::create_dir_all(&data_dir)?;

    let port = 9443; // Default daemon port

    info!("✅ Starting WebSocket server on port {}", port);
    info!("");
    info!("🎯 In another terminal, try these commands:");
    info!("   # Check daemon status");
    info!("   harness daemon status");
    info!("");
    info!("   # List services");
    info!("   harness status");
    info!("");
    info!("   # Start a service (once services are configured)");
    info!("   harness start <service-name>");
    info!("");
    info!("Press Ctrl+C to stop the daemon");

    // Start the actual WebSocket server
    harness::daemon::run(&data_dir, port).await
}