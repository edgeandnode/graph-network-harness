use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing::{info, error};

#[derive(Parser)]
#[command(name = "harness-executor-daemon")]
#[command(about = "Harness executor daemon - manages service orchestration")]
#[command(version)]
struct Cli {
    /// Port to listen on
    #[arg(short, long, default_value = "9443")]
    port: u16,
    
    /// Data directory for state and certificates
    #[arg(short, long)]
    data_dir: Option<PathBuf>,
    
    /// Regenerate TLS certificates
    #[arg(long)]
    regenerate_certs: bool,
}

fn main() -> Result<()> {
    smol::block_on(async {
    // Determine data directory early for logging
    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("harness");
    std::fs::create_dir_all(&data_dir)?;
    
    // Set up logging to file
    let log_file = data_dir.join("daemon.log");
    let file_appender = tracing_appender::rolling::never(&data_dir, "daemon.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false) // No ANSI colors in log file
        .init();
    
    let cli = Cli::parse();
    
    // Use data directory from CLI if provided, otherwise use default
    let data_dir = cli.data_dir.unwrap_or(data_dir);
    
    info!("Starting harness executor daemon");
    info!("Data directory: {:?}", data_dir);
    info!("Listen address: 127.0.0.1:{}", cli.port);
    info!("Log file: {:?}", log_file);
    
    println!("Harness executor daemon starting...");
    println!("Data directory: {:?}", data_dir);
    println!("Listen address: 127.0.0.1:{}", cli.port);
    println!("Log file: {:?}", log_file);
    
    // Data directory already created above
    
    // Handle certificate regeneration if requested
    if cli.regenerate_certs {
        info!("Regenerating TLS certificates...");
        harness::daemon::certificates::regenerate_certificates(&data_dir)?;
        info!("Certificates regenerated successfully");
        return Ok(());
    }
    
    // Start the daemon
    match harness::daemon::run(data_dir, cli.port).await {
        Ok(_) => {
            info!("Daemon shutdown gracefully");
            Ok(())
        }
        Err(e) => {
            error!("Daemon error: {}", e);
            Err(e)
        }
    }
    })
}