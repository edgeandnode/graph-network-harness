use anyhow::{Result, anyhow};

use harness::protocol::{Request, Response};

use super::client;

/// Get environment variables from the daemon
pub async fn get(names: Vec<String>) -> Result<()> {
    // Connect to daemon
    let mut daemon = client::connect_to_daemon().await?;

    // Request environment variables
    let response = daemon
        .send_request(Request::GetEnvironmentVariables {
            names: names.clone(),
        })
        .await?;

    match response {
        Response::EnvironmentVariables { variables } => {
            if variables.is_empty() {
                if names.is_empty() {
                    println!("No environment variables set in daemon");
                } else {
                    println!("None of the requested variables are set in daemon");
                }
            } else {
                // Sort for consistent output
                let mut sorted: Vec<_> = variables.into_iter().collect();
                sorted.sort_by(|a, b| a.0.cmp(&b.0));

                for (key, value) in sorted {
                    println!("{}={}", key, value);
                }
            }
            Ok(())
        }
        Response::Error { message } => {
            Err(anyhow!("Failed to get environment variables: {}", message))
        }
        _ => Err(anyhow!("Unexpected response from daemon")),
    }
}
