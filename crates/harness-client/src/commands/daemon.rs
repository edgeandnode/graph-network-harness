use anyhow::Result;

pub async fn run(command: crate::DaemonCommands) -> Result<()> {
    match command {
        crate::DaemonCommands::Status => daemon_status().await,
    }
}

async fn daemon_status() -> Result<()> {
    println!("Checking daemon status...");

    // Try to connect to daemon
    match crate::commands::client::connect_to_daemon().await {
        Ok(mut client) => {
            println!(
                "✓ Daemon is running on port {}",
                crate::commands::client::DEFAULT_DAEMON_PORT
            );
            println!("  Status: Connected");

            // Clean up connection
            client.close().await?;
            Ok(())
        }
        Err(e) => {
            println!("✗ Daemon is not reachable");
            println!("  Error: {e}");
            println!();
            println!("Note: The daemon should be started by your specific implementation");
            println!("  (e.g., graph-test-daemon, or your custom daemon)");

            // Return error so exit code is non-zero
            Err(e)
        }
    }
}
