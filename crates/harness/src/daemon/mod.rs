//! Harness executor daemon implementation

pub mod certificates;
pub mod server;
pub mod handlers;

use anyhow::Result;
use std::path::Path;

/// Run the executor daemon
pub async fn run(data_dir: impl AsRef<Path>, port: u16) -> Result<()> {
    let data_dir = data_dir.as_ref();
    
    // Ensure certificates exist and are valid
    certificates::ensure_valid_certificates(data_dir, false).await?;
    
    // Start the WebSocket server with TLS
    server::start_server(data_dir, port).await
}