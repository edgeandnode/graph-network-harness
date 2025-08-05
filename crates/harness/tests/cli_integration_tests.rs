//! Integration tests for CLI client <-> daemon interaction
//!
//! These tests simulate the full user experience from CLI command through
//! daemon response, testing the complete flow that actual users experience.

use anyhow::Result;
use std::process::Command;
use std::time::Duration;

mod common;
use common::{CliTestContext, CliOutput, find_available_port};

// ============================================================================
// Actual Integration Tests
// ============================================================================

#[tokio::test]
async fn test_cli_daemon_basic_lifecycle() -> Result<()> {
    let ctx = CliTestContext::new().await?;
    
    // Test: Check daemon status
    let output = ctx.run_cli_command(&["daemon", "status"])?;
    output
        .assert_success()
        .assert_contains("Daemon is running")
        .assert_contains(&format!("Port: {}", ctx.daemon_port));
    
    // Test: List services (should be empty)
    let output = ctx.run_cli_command(&["status"])?;
    output
        .assert_success()
        .assert_contains("No services");
    
    Ok(())
}

#[tokio::test]
async fn test_cli_service_lifecycle() -> Result<()> {
    let ctx = CliTestContext::new().await?;
    let config_path = ctx.create_test_config("test-stack")?;
    
    // Test: Start a service
    let output = ctx.run_cli_command(&[
        "start", 
        "-f", config_path.to_str().unwrap(),
        "echo-service"
    ])?;
    output
        .assert_success()
        .assert_contains("Starting service: echo-service")
        .assert_contains("Service started successfully");
    
    // Test: Check service status
    let output = ctx.run_cli_command(&["status", "echo-service"])?;
    output
        .assert_success()
        .assert_contains("echo-service")
        .assert_contains("Running");
    
    // Test: List all services
    let output = ctx.run_cli_command(&["status"])?;
    output
        .assert_success()
        .assert_contains("echo-service")
        .assert_contains("Running");
    
    // Test: Stop the service
    let output = ctx.run_cli_command(&["stop", "echo-service"])?;
    output
        .assert_success()
        .assert_contains("Stopping service: echo-service")
        .assert_contains("Service stopped successfully");
    
    // Test: Verify service is stopped
    let output = ctx.run_cli_command(&["status", "echo-service"])?;
    output
        .assert_success()
        .assert_contains("echo-service")
        .assert_contains("Stopped");
    
    Ok(())
}

#[tokio::test]
async fn test_cli_service_dependencies() -> Result<()> {
    let ctx = CliTestContext::new().await?;
    
    // Create a config with dependencies
    let config_path = ctx.test_dir.path().join("deps-stack.yaml");
    let config_content = r#"
name: deps-test
services:
  database:
    binary: echo
    args: ["Database started"]
  
  api:
    binary: echo
    args: ["API started with DB=$DATABASE_HOST:$DATABASE_PORT"]
    dependencies:
      - database
    env:
      API_PORT: "8080"
"#;
    std::fs::write(&config_path, config_content)?;
    
    // Test: Start dependent service (should start database first)
    let output = ctx.run_cli_command(&[
        "start",
        "-f", config_path.to_str().unwrap(),
        "api"
    ])?;
    output
        .assert_success()
        .assert_contains("Starting service: database") // Should start dependency first
        .assert_contains("Starting service: api");
    
    // Test: Verify both services are running
    let output = ctx.run_cli_command(&["status"])?;
    output
        .assert_success()
        .assert_contains("database")
        .assert_contains("api")
        .assert_contains("Running");
    
    // Test: Stop with dependencies
    let output = ctx.run_cli_command(&["stop", "--deps", "api"])?;
    output
        .assert_success()
        .assert_contains("Stopping service: api")
        .assert_contains("Stopping service: database");
    
    Ok(())
}

#[tokio::test]
async fn test_cli_error_handling() -> Result<()> {
    let ctx = CliTestContext::new().await?;
    
    // Test: Start non-existent service
    let output = ctx.run_cli_command(&["start", "non-existent-service"])?;
    assert!(!output.success);
    output.assert_contains("Service 'non-existent-service' not found");
    
    // Test: Stop non-existent service
    let output = ctx.run_cli_command(&["stop", "non-existent-service"])?;
    assert!(!output.success);
    output.assert_contains("Service 'non-existent-service' not found");
    
    // Test: Invalid config file
    let output = ctx.run_cli_command(&["start", "-f", "/non/existent/path.yaml", "service"])?;
    assert!(!output.success);
    output.assert_contains("Failed to read configuration");
    
    Ok(())
}

#[tokio::test]
async fn test_cli_validate_command() -> Result<()> {
    let ctx = CliTestContext::new().await?;
    let config_path = ctx.create_test_config("validate-test")?;
    
    // Test: Validate good configuration
    let output = ctx.run_cli_command(&["validate", "-f", config_path.to_str().unwrap()])?;
    output
        .assert_success()
        .assert_contains("Configuration is valid");
    
    // Test: Validate bad configuration
    let bad_config_path = ctx.test_dir.path().join("bad-config.yaml");
    let bad_config = r#"
name: bad-config
services:
  circular-a:
    binary: echo
    dependencies: [circular-b]
  circular-b:
    binary: echo
    dependencies: [circular-a]
"#;
    std::fs::write(&bad_config_path, bad_config)?;
    
    let output = ctx.run_cli_command(&["validate", "-f", bad_config_path.to_str().unwrap()])?;
    assert!(!output.success);
    output.assert_contains("Circular dependency detected");
    
    Ok(())
}

#[tokio::test]
async fn test_cli_environment_variables() -> Result<()> {
    let ctx = CliTestContext::new().await?;
    
    // Test: Use environment variable for daemon port
    std::env::set_var("HARNESS_DAEMON_PORT", ctx.daemon_port.to_string());
    let output = Command::new(&ctx.harness_binary)
        .args(&["daemon", "status"])
        .output()?;
    
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Daemon is running"));
    
    Ok(())
}

#[tokio::test]
async fn test_cli_concurrent_operations() -> Result<()> {
    let ctx = CliTestContext::new().await?;
    let config_path = ctx.create_test_config("concurrent-test")?;
    
    // Start multiple services concurrently
    let mut handles = vec![];
    for i in 0..3 {
        let harness_binary = ctx.harness_binary.clone();
        let daemon_port = ctx.daemon_port;
        let config_path = config_path.clone();
        
        let handle = tokio::spawn(async move {
            Command::new(&harness_binary)
                .env("HARNESS_DAEMON_PORT", daemon_port.to_string())
                .args(&[
                    "start",
                    "-f", config_path.to_str().unwrap(),
                    "echo-service"
                ])
                .output()
        });
        handles.push(handle);
    }
    
    // Wait for all commands to complete
    for handle in handles {
        let output = handle.await??;
        assert!(output.status.success());
    }
    
    // Verify service is running (should handle concurrent starts gracefully)
    let output = ctx.run_cli_command(&["status", "echo-service"])?;
    output
        .assert_success()
        .assert_contains("echo-service")
        .assert_contains("Running");
    
    Ok(())
}

#[tokio::test]
async fn test_cli_daemon_connection_retry() -> Result<()> {
    // Test that CLI retries connection to daemon
    let port = find_available_port()?;
    let harness_binary = std::env::current_exe()?
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("harness");
    
    // Try to connect to non-existent daemon
    let output = Command::new(&harness_binary)
        .env("HARNESS_DAEMON_PORT", port.to_string())
        .args(&["status"])
        .output()?;
    
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Failed to connect to daemon") || 
            stderr.contains("Connection refused"));
    
    Ok(())
}

// Add more tests as needed...