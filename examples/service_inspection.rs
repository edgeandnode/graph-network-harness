//! Example demonstrating service inspection capabilities

use local_network_harness::{
    LocalNetworkHarness, HarnessConfig,
};
use clap::Parser;
use std::path::PathBuf;
use tracing::info;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "service_inspection")]
#[command(about = "Demonstrates service inspection capabilities with local network")]
struct Args {
    /// Path to local-network directory (e.g., submodules/local-network)
    #[arg(long, value_name = "PATH")]
    local_network: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse command line arguments
    let args = Args::parse();
    
    // Initialize logging
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();

    info!("Starting service inspection example");
    info!("Using local-network at: {}", args.local_network);

    // Create harness configuration
    let config = HarnessConfig {
        local_network_path: PathBuf::from(args.local_network),
        session_name: Some("service-inspection-demo".to_string()),
        startup_timeout: Duration::from_secs(120),
        auto_sync_images: true,
        build_images: false,
        ..Default::default()
    };

    // Create and start the harness
    let mut harness = LocalNetworkHarness::new(config)?;
    info!("Starting local network harness...");
    harness.start().await?;

    // Start the local network services
    info!("Starting local network services...");
    match harness.start_local_network().await {
        Ok(_) => info!("Local network started successfully"),
        Err(e) => {
            info!("Warning: Failed to start all services: {}", e);
            info!("Continuing with available services...");
        }
    }

    // Give services a moment to start
    info!("Waiting for services to stabilize...");
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Create service inspector and check running containers
    info!("Starting service inspection...");
    let inspector = harness.create_service_inspector()?;
    let containers = harness.get_running_containers().await?;
    
    info!("Found {} running containers:", containers.len());
    for (name, id) in &containers {
        info!("  ✓ {} ({})", name, &id[..12]);
    }
    
    // Show inspection capabilities
    info!("");
    info!("Service inspector is configured with handlers for:");
    info!("  • postgres - Database events and errors");
    info!("  • graph-node - Indexing and sync events");
    info!("  • Generic fallback - Error/warning detection for all services");
    
    // In a real application, you would:
    // 1. Call inspector.start_streaming(containers) to begin monitoring
    // 2. Use inspector.event_stream() to get a Stream of ServiceEvent
    // 3. Process events in real-time for monitoring, alerting, etc.
    
    info!("");
    info!("Example inspection queries you could run:");
    info!("  • Stream all events: inspector.event_stream()");
    info!("  • Stream postgres events: inspector.service_stream(\"postgres\")");
    info!("  • Filter by severity: stream.filter(|e| e.is_error())");

    // Check some service logs manually
    info!("");
    info!("Checking service status...");
    
    // Check postgres
    let postgres_logs = harness.exec(
        vec!["docker", "logs", "--tail", "10", "postgres"], 
        None
    ).await?;
    
    if postgres_logs == 0 {
        info!("✓ PostgreSQL logs retrieved successfully");
    }
    
    // Check graph-node  
    let graph_logs = harness.exec(
        vec!["docker", "logs", "--tail", "10", "graph-node"],
        None
    ).await?;
    
    if graph_logs == 0 {
        info!("✓ Graph Node logs retrieved successfully");
    }

    // Stop the local network
    info!("");
    info!("Stopping local network...");
    harness.stop_local_network().await?;

    // Print log summary
    harness.print_log_summary();

    info!("");
    info!("Service inspection example complete!");
    info!("Check the log files in test-activity/logs/ for full details");
    Ok(())
}