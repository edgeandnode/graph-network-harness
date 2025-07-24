use anyhow::{anyhow, Result};
use std::collections::HashMap;

use harness::protocol::{Request, Response};

use super::client;

/// Set environment variables in the daemon
pub async fn set(variables: Vec<String>) -> Result<()> {
    // Parse KEY=VALUE pairs
    let mut env_vars = HashMap::new();
    for var in variables {
        let parts: Vec<&str> = var.splitn(2, '=').collect();
        if parts.len() != 2 {
            return Err(anyhow!(
                "Invalid environment variable format: '{}'. Expected KEY=VALUE",
                var
            ));
        }
        env_vars.insert(parts[0].to_string(), parts[1].to_string());
    }

    // Connect to daemon
    let mut daemon = client::connect_to_daemon().await?;

    // Send environment variables to daemon
    let response = daemon
        .send_request(Request::SetEnvironmentVariables {
            variables: env_vars.clone(),
        })
        .await?;

    match response {
        Response::Success => {
            println!("✓ Set {} environment variables in daemon", env_vars.len());
            for (key, _) in env_vars {
                println!("  - {}", key);
            }
            Ok(())
        }
        Response::Error { message } => Err(anyhow!("Failed to set environment variables: {}", message)),
        _ => Err(anyhow!("Unexpected response from daemon")),
    }
}

/// Get environment variables from the daemon
pub async fn get(names: Vec<String>) -> Result<()> {
    // Connect to daemon
    let mut daemon = client::connect_to_daemon().await?;

    // Request environment variables
    let response = daemon
        .send_request(Request::GetEnvironmentVariables { names: names.clone() })
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
        Response::Error { message } => Err(anyhow!("Failed to get environment variables: {}", message)),
        _ => Err(anyhow!("Unexpected response from daemon")),
    }
}