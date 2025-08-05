//! User scenario tests for CLI
//!
//! These tests simulate real-world user workflows and scenarios

use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

mod common;
use common::CliTestContext;

#[tokio::test]
async fn test_user_scenario_graph_protocol_stack() -> Result<()> {
    let ctx = CliTestContext::new().await?;

    // Create a realistic Graph Protocol stack configuration
    let config_path = ctx.test_dir.path().join("graph-stack.yaml");
    let config_content = r#"
name: graph-local
services:
  postgres:
    binary: echo
    args: ["PostgreSQL 14.5 on port 5432"]
    env:
      PGPORT: "5432"
      POSTGRES_DB: "graph-node"
  
  ipfs:
    binary: echo
    args: ["IPFS daemon on port 5001"]
    env:
      IPFS_API_PORT: "5001"
      IPFS_GATEWAY_PORT: "8080"
  
  ethereum-node:
    binary: echo
    args: ["Ethereum node (anvil) on port 8545"]
    env:
      CHAIN_ID: "1337"
      RPC_PORT: "8545"
  
  graph-node:
    binary: echo
    args: ["Graph Node started"]
    dependencies:
      - postgres
      - ipfs
      - ethereum-node
    env:
      GRAPH_NODE_PORT: "8020"
      GRAPH_INDEXER_PORT: "8030"

tasks:
  deploy-contracts:
    binary: echo
    args: ["Deploying Graph Protocol contracts..."]
    dependencies:
      - ethereum-node
  
  deploy-subgraph:
    binary: echo
    args: ["Deploying example subgraph..."]
    dependencies:
      - graph-node
      - deploy-contracts
"#;
    std::fs::write(&config_path, config_content)?;

    // User scenario: Start the entire Graph Protocol stack
    println!("=== Starting Graph Protocol Stack ===");

    // Step 1: Validate configuration
    let output = ctx.run_cli_command(&["validate", "-f", config_path.to_str().unwrap()])?;
    output
        .assert_success()
        .assert_contains("Configuration is valid");
    println!("✓ Configuration validated");

    // Step 2: Check initial status
    let output = ctx.run_cli_command(&["status"])?;
    output.assert_success().assert_contains("No services");
    println!("✓ No services running initially");

    // Step 3: Start the graph-node (should start all dependencies)
    let output =
        ctx.run_cli_command(&["start", "-f", config_path.to_str().unwrap(), "graph-node"])?;
    output
        .assert_success()
        .assert_contains("Starting service: postgres")
        .assert_contains("Starting service: ipfs")
        .assert_contains("Starting service: ethereum-node")
        .assert_contains("Starting service: graph-node");
    println!("✓ Started graph-node with all dependencies");

    // Step 4: Check all services are running
    let output = ctx.run_cli_command(&["status"])?;
    output
        .assert_success()
        .assert_contains("postgres")
        .assert_contains("ipfs")
        .assert_contains("ethereum-node")
        .assert_contains("graph-node");

    // Verify all are running
    let status_lines: Vec<&str> = output.stdout.lines().collect();
    let running_count = status_lines
        .iter()
        .filter(|line| line.contains("Running"))
        .count();
    assert_eq!(running_count, 4, "All 4 services should be running");
    println!("✓ All services running");

    // Step 5: Deploy contracts task
    let output = ctx.run_cli_command(&[
        "start",
        "-f",
        config_path.to_str().unwrap(),
        "--task",
        "deploy-contracts",
    ])?;
    output
        .assert_success()
        .assert_contains("Running task: deploy-contracts")
        .assert_contains("Task completed successfully");
    println!("✓ Deployed contracts");

    // Step 6: Deploy subgraph task
    let output = ctx.run_cli_command(&[
        "start",
        "-f",
        config_path.to_str().unwrap(),
        "--task",
        "deploy-subgraph",
    ])?;
    output
        .assert_success()
        .assert_contains("Running task: deploy-subgraph")
        .assert_contains("Task completed successfully");
    println!("✓ Deployed subgraph");

    // Step 7: User realizes they need to restart graph-node
    let output = ctx.run_cli_command(&["restart", "graph-node"])?;
    output
        .assert_success()
        .assert_contains("Restarting service: graph-node");
    println!("✓ Restarted graph-node");

    // Step 8: Stop everything
    let output = ctx.run_cli_command(&["stop", "--all"])?;
    output
        .assert_success()
        .assert_contains("Stopping all services");
    println!("✓ Stopped all services");

    Ok(())
}

#[tokio::test]
async fn test_user_scenario_development_workflow() -> Result<()> {
    let ctx = CliTestContext::new().await?;

    // Create a development environment config
    let config_path = ctx.test_dir.path().join("dev-env.yaml");
    let config_content = r#"
name: development
services:
  database:
    binary: echo
    args: ["Development database on 5432"]
    env:
      DB_PORT: "5432"
  
  api:
    binary: echo
    args: ["API server on 3000"]
    dependencies: [database]
    env:
      API_PORT: "3000"
      DATABASE_URL: "postgres://localhost:5432/dev"
  
  frontend:
    binary: echo
    args: ["Frontend dev server on 3001"]
    dependencies: [api]
    env:
      FRONTEND_PORT: "3001"
      API_URL: "http://localhost:3000"
"#;
    std::fs::write(&config_path, config_content)?;

    println!("=== Developer Daily Workflow ===");

    // Morning: Start development environment
    println!("\n--- Morning: Starting work ---");
    let output = ctx.run_cli_command(&["start", "-f", config_path.to_str().unwrap(), "--all"])?;
    output.assert_success();
    println!("✓ Started development environment");

    // Check what's running
    let output = ctx.run_cli_command(&["status", "--detailed"])?;
    output
        .assert_success()
        .assert_contains("database")
        .assert_contains("api")
        .assert_contains("frontend");
    println!("✓ All services healthy");

    // Midday: API needs restart after code changes
    println!("\n--- Midday: Restarting API after changes ---");
    let output = ctx.run_cli_command(&["restart", "api"])?;
    output
        .assert_success()
        .assert_contains("Restarting service: api");
    println!("✓ API restarted");

    // Afternoon: Need to debug database, stop frontend temporarily
    println!("\n--- Afternoon: Debugging database ---");
    let output = ctx.run_cli_command(&["stop", "frontend"])?;
    output.assert_success();
    println!("✓ Stopped frontend for debugging");

    let output = ctx.run_cli_command(&["logs", "database", "--tail", "50"])?;
    // In real implementation, this would show actual logs
    println!("✓ Checked database logs");

    // Resume frontend
    let output = ctx.run_cli_command(&["start", "frontend"])?;
    output.assert_success();
    println!("✓ Resumed frontend");

    // Evening: Stop work
    println!("\n--- Evening: Stopping work ---");
    let output = ctx.run_cli_command(&["stop", "--all"])?;
    output
        .assert_success()
        .assert_contains("Stopping all services");
    println!("✓ Stopped all services, heading home!");

    Ok(())
}

#[tokio::test]
async fn test_user_scenario_troubleshooting() -> Result<()> {
    let ctx = CliTestContext::new().await?;

    // Create a config with a problematic service
    let config_path = ctx.test_dir.path().join("problematic.yaml");
    let config_content = r#"
name: troubleshooting
services:
  healthy-service:
    binary: echo
    args: ["I'm working fine"]
  
  flaky-service:
    binary: false  # This will fail
    args: []
    dependencies: [healthy-service]
  
  dependent-service:
    binary: echo
    args: ["I depend on flaky-service"]
    dependencies: [flaky-service]
"#;
    std::fs::write(&config_path, config_content)?;

    println!("=== Troubleshooting Scenario ===");

    // Try to start all services
    let output = ctx.run_cli_command(&["start", "-f", config_path.to_str().unwrap(), "--all"])?;

    // Should partially succeed
    println!("✓ Attempted to start all services");

    // Check status to see what's wrong
    let output = ctx.run_cli_command(&["status", "--detailed"])?;
    output
        .assert_success()
        .assert_contains("healthy-service")
        .assert_contains("Running");
    println!("✓ Healthy service is running");

    // flaky-service should have failed
    // dependent-service should not have started

    // Try to get more info about the failure
    let output = ctx.run_cli_command(&["logs", "flaky-service"])?;
    println!("✓ Checked logs for failed service");

    // Fix the issue (update config)
    let fixed_config = r#"
name: troubleshooting
services:
  healthy-service:
    binary: echo
    args: ["I'm working fine"]
  
  flaky-service:
    binary: echo  # Fixed!
    args: ["Now I work"]
    dependencies: [healthy-service]
  
  dependent-service:
    binary: echo
    args: ["I depend on flaky-service"]
    dependencies: [flaky-service]
"#;
    std::fs::write(&config_path, fixed_config)?;

    // Retry with fixed config
    println!("\n--- After fixing configuration ---");
    let output = ctx.run_cli_command(&[
        "start",
        "-f",
        config_path.to_str().unwrap(),
        "flaky-service",
    ])?;
    output
        .assert_success()
        .assert_contains("Starting service: flaky-service");
    println!("✓ Fixed service now starts");

    // Now start the dependent
    let output = ctx.run_cli_command(&[
        "start",
        "-f",
        config_path.to_str().unwrap(),
        "dependent-service",
    ])?;
    output.assert_success();
    println!("✓ Dependent service now works");

    Ok(())
}

#[tokio::test]
async fn test_user_scenario_multiple_environments() -> Result<()> {
    let ctx = CliTestContext::new().await?;

    // Create configs for different environments
    let dev_config = ctx.test_dir.path().join("dev.yaml");
    std::fs::write(
        &dev_config,
        r#"
name: dev-environment
services:
  dev-db:
    binary: echo
    args: ["Dev DB on 5432"]
    env:
      ENV: "development"
"#,
    )?;

    let staging_config = ctx.test_dir.path().join("staging.yaml");
    std::fs::write(
        &staging_config,
        r#"
name: staging-environment  
services:
  staging-db:
    binary: echo
    args: ["Staging DB on 5433"]
    env:
      ENV: "staging"
"#,
    )?;

    println!("=== Multiple Environments Scenario ===");

    // Start development environment
    let output = ctx.run_cli_command(&["start", "-f", dev_config.to_str().unwrap(), "--all"])?;
    output.assert_success();
    println!("✓ Started development environment");

    // Start staging environment
    let output =
        ctx.run_cli_command(&["start", "-f", staging_config.to_str().unwrap(), "--all"])?;
    output.assert_success();
    println!("✓ Started staging environment");

    // Check both are running
    let output = ctx.run_cli_command(&["status"])?;
    output
        .assert_success()
        .assert_contains("dev-db")
        .assert_contains("staging-db");
    println!("✓ Both environments running simultaneously");

    // Stop just staging
    let output = ctx.run_cli_command(&["stop", "staging-db"])?;
    output.assert_success();
    println!("✓ Stopped staging while keeping dev running");

    // Verify dev is still running
    let output = ctx.run_cli_command(&["status", "dev-db"])?;
    output.assert_success().assert_contains("Running");
    println!("✓ Development environment unaffected");

    Ok(())
}
