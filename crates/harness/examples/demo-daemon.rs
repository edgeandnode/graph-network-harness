//! Demo daemon - simulates a microservices platform for testing
//!
//! Run with: cargo run --example demo-daemon --features smol

use anyhow::Result;
use harness_core::daemon::{Daemon, DaemonBuilder};
use tracing::info;

fn main() -> Result<()> {
    // Use smol runtime
    smol::block_on(async { run_daemon().await })
}

async fn run_daemon() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    info!("🚀 Starting Demo Daemon");
    info!("📦 This simulates a simple microservices platform");

    // Create daemon builder
    let builder = DaemonBuilder::new()
        .with_endpoint("127.0.0.1:8090".parse()?)
        .with_state_dir("./demo-daemon-data");

    // For now, just build a basic daemon
    // We'll add services and tasks once the basic structure is working
    let daemon = builder.build().await?;

    info!("✅ Demo daemon configured!");
    info!("");
    info!("🎯 The daemon is running on port 8090");
    info!("   Note: This is a basic demo. Services and tasks will be added next.");

    // Start the daemon
    daemon.start().await?;

    info!("");
    info!("⚠️  Note: The WebSocket server is not yet implemented in BaseDaemon");
    info!("    This demo will run for 10 seconds and then exit.");
    info!("");
    info!("    In a real implementation, you would:");
    info!("    - Use harness CLI to connect: harness --daemon-port 8090 status");
    info!("    - Or press Ctrl+C to stop the daemon");

    // Run for 10 seconds as a demo
    smol::Timer::after(std::time::Duration::from_secs(10)).await;

    info!("");
    info!("Demo completed. Stopping daemon...");
    daemon.stop().await?;

    Ok(())
}
