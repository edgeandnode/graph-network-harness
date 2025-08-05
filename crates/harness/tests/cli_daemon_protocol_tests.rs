//! Tests for CLI-daemon protocol and communication
//!
//! These tests focus on the WebSocket protocol, error handling,
//! and edge cases in client-daemon communication.

use anyhow::Result;
use std::time::Duration;

mod common;
use common::{CliTestContext, find_available_port};

#[tokio::test]
async fn test_daemon_websocket_connection() -> Result<()> {
    let ctx = CliTestContext::new().await?;

    // Test: Basic connection works
    let output = ctx.run_cli_command(&["daemon", "status"])?;
    output.assert_success().assert_contains("Daemon is running");

    // Test: Multiple clients can connect simultaneously
    let mut handles = vec![];
    for i in 0..5 {
        let ctx_clone = &ctx;
        let handle = tokio::task::spawn_blocking(move || ctx_clone.run_cli_command(&["status"]));
        handles.push(handle);
    }

    // All should succeed
    for (i, handle) in handles.into_iter().enumerate() {
        let output = handle.await??;
        output.assert_success();
        println!("Client {} connected successfully", i);
    }

    Ok(())
}

#[tokio::test]
async fn test_daemon_connection_failure_handling() -> Result<()> {
    // Test connecting to non-existent daemon
    let port = find_available_port()?;
    let ctx = CliTestContext::new().await?;

    // Override port to non-existent daemon
    let output =
        ctx.run_cli_command_with_env(&["status"], &[("HARNESS_DAEMON_PORT", &port.to_string())])?;

    output
        .assert_failure()
        .assert_contains("Failed to connect to daemon");

    Ok(())
}

#[tokio::test]
async fn test_daemon_tls_communication() -> Result<()> {
    // Start daemon with TLS enabled
    let ctx = CliTestContext::with_daemon_args(&["--tls"]).await?;

    // Commands should still work with TLS
    let output = ctx.run_cli_command(&["status"])?;
    output.assert_success().assert_contains("No services");

    // Verify we're using TLS (daemon status should indicate this)
    let output = ctx.run_cli_command(&["daemon", "status"])?;
    output.assert_success().assert_contains("TLS: enabled");

    Ok(())
}

#[tokio::test]
async fn test_large_response_handling() -> Result<()> {
    let ctx = CliTestContext::new().await?;

    // Create many services to generate a large response
    for i in 0..20 {
        let config_content = format!(
            r#"
name: bulk-test-{}
services:
  service-{}:
    binary: echo
    args: ["Service {} with a long description that makes the response larger"]
    env:
      VAR1: "value1"
      VAR2: "value2"
      VAR3: "value3"
"#,
            i, i, i
        );

        let config_path = ctx.create_config(&format!("bulk-{}.yaml", i), &config_content)?;

        // Start the service
        ctx.run_cli_command(&[
            "start",
            "-f",
            config_path.to_str().unwrap(),
            &format!("service-{}", i),
        ])?;
    }

    // Now list all services (large response)
    let output = ctx.run_cli_command(&["status", "--detailed"])?;
    output.assert_success();

    // Verify all services are listed
    for i in 0..20 {
        output.assert_contains(&format!("service-{}", i));
    }

    Ok(())
}

#[tokio::test]
async fn test_concurrent_modifications() -> Result<()> {
    let ctx = CliTestContext::new().await?;
    let config_path = ctx.create_test_config("concurrent")?;

    // Start a service
    ctx.run_cli_command(&["start", "-f", config_path.to_str().unwrap(), "echo-service"])?
        .assert_success();

    // Concurrently try to stop and restart the service
    let ctx_ref = &ctx;
    let config_str = config_path.to_str().unwrap();

    let stop_handle =
        tokio::task::spawn_blocking(move || ctx_ref.run_cli_command(&["stop", "echo-service"]));

    let restart_handle = tokio::task::spawn_blocking(move || {
        std::thread::sleep(Duration::from_millis(10)); // Small delay
        ctx_ref.run_cli_command(&["restart", "echo-service"])
    });

    // Both operations should complete (one might fail due to race)
    let stop_result = stop_handle.await?;
    let restart_result = restart_handle.await?;

    // At least one should succeed
    assert!(
        stop_result.is_ok() || restart_result.is_ok(),
        "Both concurrent operations failed"
    );

    Ok(())
}

#[tokio::test]
async fn test_daemon_graceful_shutdown() -> Result<()> {
    let ctx = CliTestContext::new().await?;

    // Start a service
    let config_path = ctx.create_test_config("shutdown-test")?;
    ctx.run_cli_command(&["start", "-f", config_path.to_str().unwrap(), "echo-service"])?
        .assert_success();

    // Stop daemon gracefully
    let output = ctx.run_cli_command(&["daemon", "stop"])?;
    output.assert_success().assert_contains("Daemon stopped");

    // Verify daemon is no longer accessible
    tokio::time::sleep(Duration::from_millis(500)).await;

    let output = ctx.run_cli_command(&["status"])?;
    output.assert_failure().assert_contains("Failed to connect");

    Ok(())
}

#[tokio::test]
async fn test_request_timeout_handling() -> Result<()> {
    let ctx = CliTestContext::new().await?;

    // Create a config that would cause a slow operation
    let config_content = r#"
name: timeout-test
services:
  slow-service:
    binary: sleep
    args: ["10"]  # Sleep for 10 seconds
"#;

    let config_path = ctx.create_config("timeout.yaml", config_content)?;

    // Start the slow service (this should not timeout the request itself)
    let output =
        ctx.run_cli_command(&["start", "-f", config_path.to_str().unwrap(), "slow-service"])?;

    output
        .assert_success()
        .assert_contains("Starting service: slow-service");

    Ok(())
}

#[tokio::test]
async fn test_malformed_config_handling() -> Result<()> {
    let ctx = CliTestContext::new().await?;

    // Test various malformed configurations
    let bad_configs = vec![
        // Invalid YAML
        ("invalid.yaml", "this is not valid yaml: {"),
        // Missing required fields
        (
            "missing-fields.yaml",
            r#"
name: test
services:
  bad-service:
    # Missing binary field
    args: ["test"]
"#,
        ),
        // Invalid service name
        (
            "bad-name.yaml",
            r#"
name: test
services:
  "service with spaces":
    binary: echo
"#,
        ),
    ];

    for (filename, content) in bad_configs {
        let config_path = ctx.create_config(filename, content)?;

        let output = ctx.run_cli_command(&["validate", "-f", config_path.to_str().unwrap()])?;

        output.assert_failure();
        println!("✓ Correctly rejected {}", filename);
    }

    Ok(())
}

#[tokio::test]
async fn test_event_streaming() -> Result<()> {
    let ctx = CliTestContext::new().await?;

    // This test would require implementing a "watch" or "events" command
    // that streams events from the daemon

    // For now, we can test that services generate events
    let config_path = ctx.create_test_config("events-test")?;

    // Start service
    ctx.run_cli_command(&["start", "-f", config_path.to_str().unwrap(), "echo-service"])?
        .assert_success();

    // In a real implementation, we would:
    // 1. Start an event stream listener
    // 2. Perform operations (start/stop/restart)
    // 3. Verify events are received

    // Stop service
    ctx.run_cli_command(&["stop", "echo-service"])?
        .assert_success();

    Ok(())
}

#[tokio::test]
async fn test_daemon_state_persistence() -> Result<()> {
    // Create a persistent state directory
    let state_dir = tempfile::tempdir()?;
    let state_path = state_dir.path().to_path_buf();

    // Start daemon with specific state directory
    let mut ctx =
        CliTestContext::with_daemon_args(&["--state-dir", state_path.to_str().unwrap()]).await?;

    // Start a service
    let config_path = ctx.create_test_config("persistent")?;
    ctx.run_cli_command(&["start", "-f", config_path.to_str().unwrap(), "echo-service"])?
        .assert_success();

    // Get the daemon port before dropping context
    let daemon_port = ctx.daemon_port;

    // Stop daemon (drop context)
    drop(ctx);
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Start new daemon with same state directory
    let new_ctx = CliTestContext::with_daemon_args(&[
        "--state-dir",
        state_path.to_str().unwrap(),
        "--port",
        &daemon_port.to_string(),
    ])
    .await?;

    // Service should still be registered (though not running)
    let output = new_ctx.run_cli_command(&["status"])?;
    output.assert_success().assert_contains("echo-service");

    Ok(())
}
