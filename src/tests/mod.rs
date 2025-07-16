pub mod contracts;
pub mod database;
pub mod graph_node;
pub mod end_to_end;

use crate::harness::LocalNetworkHarness;
use anyhow::Result;
use tracing::info;

/// Common test utilities and setup
pub async fn setup_test_environment(harness: &mut LocalNetworkHarness) -> Result<()> {
    info!("Setting up test environment");
    
    // Ensure the harness is running
    harness.ensure_running().await?;
    
    // Wait for services to be ready
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    
    Ok(())
}

/// Common test cleanup
pub async fn cleanup_test_environment(_harness: &LocalNetworkHarness) -> Result<()> {
    info!("Cleaning up test environment");
    
    // Clean up any test data if needed
    // For now, we rely on the harness lifecycle management
    
    Ok(())
}