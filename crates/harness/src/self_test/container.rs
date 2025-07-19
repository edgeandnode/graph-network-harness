//! Self-tests for container management functionality
//!
//! These tests verify that the Docker-in-Docker container management,
//! command execution, and logging infrastructure work correctly.

use crate::container::{ContainerConfig, DindManager};
use crate::self_test::helpers::require_docker;
use anyhow::Result;
use tempfile::TempDir;
use tokio::fs;
use tracing::info;

#[tokio::test]
async fn test_dind_container_lifecycle() -> Result<()> {
    require_docker!();

    // Create a temporary directory for logs
    let temp_dir = TempDir::new()?;
    let log_dir = temp_dir.path().join("logs");

    // Get the actual docker-test-env path relative to the project root
    let current_dir = std::env::current_dir()?;
    let docker_test_env_path = current_dir.join("docker-test-env");

    if !docker_test_env_path.exists() {
        eprintln!(
            "Skipping test: docker-test-env not found at {:?}",
            docker_test_env_path
        );
        return Ok(());
    }

    let config = ContainerConfig {
        docker_test_env_path,
        project_root: current_dir,
        log_dir: log_dir.clone(),
        startup_timeout: std::time::Duration::from_secs(120), // Longer timeout for first run
        ..ContainerConfig::default()
    };

    let mut manager = DindManager::new(config)?;
    info!("Created DindManager with session: {}", manager.session_id());

    // Test ensuring container is running
    info!("Starting DinD container...");
    manager.ensure_running().await?;
    info!("Container started successfully");

    // Test executing a simple command
    info!("Testing echo command...");
    let exit_code = manager
        .exec_in_container(vec!["echo", "Hello from DinD"], None)
        .await?;
    assert_eq!(exit_code, 0, "Echo command should succeed");

    // Test executing docker version inside container
    info!("Testing docker version inside container...");
    let exit_code = manager
        .exec_in_container(vec!["docker", "version"], None)
        .await?;
    assert_eq!(
        exit_code, 0,
        "Docker should be available inside DinD container"
    );

    // Verify log files were created
    let session_id = manager.session_id();
    let compose_log = log_dir.join(format!("{}_docker-compose.log", session_id));
    let echo_log = log_dir.join(format!("{}_exec_echo.log", session_id));
    let docker_log = log_dir.join(format!("{}_exec_docker.log", session_id));

    assert!(compose_log.exists(), "Docker compose log should exist");
    assert!(echo_log.exists(), "Echo command log should exist");
    assert!(docker_log.exists(), "Docker command log should exist");

    // Read and verify log content
    let compose_content = fs::read_to_string(&compose_log).await?;
    assert!(
        compose_content.contains("[") && compose_content.contains("]"),
        "Log should have timestamp brackets"
    );

    let echo_content = fs::read_to_string(&echo_log).await?;
    assert!(
        echo_content.contains("Hello from DinD"),
        "Echo output should be in log"
    );
    assert!(
        echo_content.contains("[stdout]"),
        "Log should indicate stdout"
    );

    // Print summary for debugging
    manager.print_log_summary();

    Ok(())
}

#[tokio::test]
async fn test_container_multiple_commands() -> Result<()> {
    require_docker!();

    let temp_dir = TempDir::new()?;
    let log_dir = temp_dir.path().join("logs");

    let current_dir = std::env::current_dir()?;
    let docker_test_env_path = current_dir.join("integration-tests/docker-test-env");

    if !docker_test_env_path.exists() {
        eprintln!("Skipping test: docker-test-env not found");
        return Ok(());
    }

    let config = ContainerConfig {
        docker_test_env_path,
        project_root: current_dir,
        log_dir: log_dir.clone(),
        ..ContainerConfig::default()
    };

    let mut manager = DindManager::new(config)?;
    manager.ensure_running().await?;

    // Execute multiple commands
    let commands = vec![
        (vec!["pwd"], "/workspace"),
        (vec!["ls", "-la"], "/workspace"),
        (vec!["docker", "ps"], "/workspace"),
    ];

    for (cmd, workdir) in commands {
        info!("Executing: {:?} in {}", cmd, workdir);
        let exit_code = manager
            .exec_in_container(cmd.clone(), Some(workdir))
            .await?;
        assert_eq!(exit_code, 0, "Command {:?} should succeed", cmd);
    }

    // Verify all log files exist
    let session_id = manager.session_id();
    let pwd_log = log_dir.join(format!("{}_exec_pwd.log", session_id));
    let ls_log = log_dir.join(format!("{}_exec_ls.log", session_id));
    let ps_log = log_dir.join(format!("{}_exec_docker.log", session_id));

    assert!(pwd_log.exists(), "pwd log should exist");
    assert!(ls_log.exists(), "ls log should exist");
    assert!(ps_log.exists(), "docker ps log should exist");

    // Verify working directory was used
    let pwd_content = fs::read_to_string(&pwd_log).await?;
    assert!(
        pwd_content.contains("/workspace"),
        "pwd should show /workspace"
    );

    Ok(())
}

#[tokio::test]
async fn test_log_streaming_multiline() -> Result<()> {
    require_docker!();

    let temp_dir = TempDir::new()?;
    let log_dir = temp_dir.path().join("logs");

    let current_dir = std::env::current_dir()?;
    let docker_test_env_path = current_dir.join("integration-tests/docker-test-env");

    if !docker_test_env_path.exists() {
        eprintln!("Skipping test: docker-test-env not found");
        return Ok(());
    }

    let config = ContainerConfig {
        docker_test_env_path,
        project_root: current_dir,
        log_dir: log_dir.clone(),
        ..ContainerConfig::default()
    };

    let mut manager = DindManager::new(config)?;
    manager.ensure_running().await?;

    // Execute a command that produces multiple lines with timestamps
    let script = r#"
        echo "Starting multi-line output test"
        for i in 1 2 3 4 5; do
            echo "Line $i at $(date +%T)"
            sleep 0.1
        done
        echo "Test completed"
    "#;

    let exit_code = manager
        .exec_in_container(vec!["sh", "-c", script], None)
        .await?;
    assert_eq!(exit_code, 0);

    // Verify the log file contains all lines with proper timestamps
    let session_id = manager.session_id();
    let exec_log = log_dir.join(format!("{}_exec_sh.log", session_id));
    let content = fs::read_to_string(&exec_log).await?;

    // Check that all lines are present
    assert!(content.contains("Starting multi-line output test"));
    for i in 1..=5 {
        assert!(
            content.contains(&format!("Line {}", i)),
            "Line {} should be in log",
            i
        );
    }
    assert!(content.contains("Test completed"));

    // Verify timestamp format - should have multiple timestamped entries
    let timestamp_count = content.matches("] [stdout]").count();
    assert!(
        timestamp_count >= 7,
        "Should have at least 7 timestamped log entries, found {}",
        timestamp_count
    );

    // Verify proper timestamp format
    assert!(
        content.contains("[2025-") || content.contains("[2024-"),
        "Should have year in timestamp"
    );

    Ok(())
}

#[tokio::test]
async fn test_stderr_capture() -> Result<()> {
    require_docker!();

    let temp_dir = TempDir::new()?;
    let log_dir = temp_dir.path().join("logs");

    let current_dir = std::env::current_dir()?;
    let docker_test_env_path = current_dir.join("integration-tests/docker-test-env");

    if !docker_test_env_path.exists() {
        eprintln!("Skipping test: docker-test-env not found");
        return Ok(());
    }

    let config = ContainerConfig {
        docker_test_env_path,
        project_root: current_dir,
        log_dir: log_dir.clone(),
        ..ContainerConfig::default()
    };

    let mut manager = DindManager::new(config)?;
    manager.ensure_running().await?;

    // Execute a command that writes to both stdout and stderr
    let script = r#"
        echo "This goes to stdout"
        echo "This goes to stderr" >&2
        echo "Another stdout line"
        echo "Another stderr line" >&2
    "#;

    let exit_code = manager
        .exec_in_container(vec!["sh", "-c", script], None)
        .await?;
    assert_eq!(exit_code, 0);

    // Read the log file
    let session_id = manager.session_id();
    let exec_log = log_dir.join(format!("{}_exec_sh.log", session_id));
    let content = fs::read_to_string(&exec_log).await?;

    // Verify both stdout and stderr are captured
    assert!(content.contains("[stdout] This goes to stdout"));
    assert!(content.contains("[stderr] This goes to stderr"));
    assert!(content.contains("[stdout] Another stdout line"));
    assert!(content.contains("[stderr] Another stderr line"));

    Ok(())
}

#[tokio::test]
async fn test_failed_command() -> Result<()> {
    require_docker!();

    let temp_dir = TempDir::new()?;
    let log_dir = temp_dir.path().join("logs");

    let current_dir = std::env::current_dir()?;
    let docker_test_env_path = current_dir.join("integration-tests/docker-test-env");

    if !docker_test_env_path.exists() {
        eprintln!("Skipping test: docker-test-env not found");
        return Ok(());
    }

    let config = ContainerConfig {
        docker_test_env_path,
        project_root: current_dir,
        log_dir: log_dir.clone(),
        ..ContainerConfig::default()
    };

    let mut manager = DindManager::new(config)?;
    manager.ensure_running().await?;

    // Execute a command that fails
    let exit_code = manager
        .exec_in_container(vec!["sh", "-c", "echo 'About to fail' && exit 42"], None)
        .await?;
    assert_eq!(exit_code, 42, "Command should exit with code 42");

    // Verify the log captures the output and exit code
    let session_id = manager.session_id();
    let exec_log = log_dir.join(format!("{}_exec_sh.log", session_id));
    let content = fs::read_to_string(&exec_log).await?;

    assert!(content.contains("About to fail"));
    assert!(content.contains("Process exited with code: 42"));

    Ok(())
}

#[tokio::test]
async fn test_concurrent_log_writing() -> Result<()> {
    require_docker!();

    let temp_dir = TempDir::new()?;
    let log_dir = temp_dir.path().join("logs");

    let current_dir = std::env::current_dir()?;
    let docker_test_env_path = current_dir.join("integration-tests/docker-test-env");

    if !docker_test_env_path.exists() {
        eprintln!("Skipping test: docker-test-env not found");
        return Ok(());
    }

    let config = ContainerConfig {
        docker_test_env_path,
        project_root: current_dir,
        log_dir: log_dir.clone(),
        ..ContainerConfig::default()
    };

    let mut manager = DindManager::new(config)?;
    manager.ensure_running().await?;

    // Execute a command that produces interleaved stdout/stderr
    let script = r#"
        for i in 1 2 3 4 5; do
            echo "stdout $i"
            echo "stderr $i" >&2
        done
    "#;

    let exit_code = manager
        .exec_in_container(vec!["sh", "-c", script], None)
        .await?;
    assert_eq!(exit_code, 0);

    // Verify the log has both streams properly tagged
    let session_id = manager.session_id();
    let exec_log = log_dir.join(format!("{}_exec_sh.log", session_id));
    let content = fs::read_to_string(&exec_log).await?;

    // Check all outputs are present
    for i in 1..=5 {
        assert!(content.contains(&format!("[stdout] stdout {}", i)));
        assert!(content.contains(&format!("[stderr] stderr {}", i)));
    }

    // Verify timestamps are properly formatted and sequential
    let lines: Vec<&str> = content.lines().collect();
    assert!(lines.len() >= 10, "Should have at least 10 log lines");

    for line in &lines {
        if !line.is_empty() && !line.contains("Process exited") {
            assert!(
                line.starts_with("[20"),
                "Line should start with timestamp: {}",
                line
            );
        }
    }

    Ok(())
}

/// Helper to create a test config with custom timeout
pub fn test_config_with_timeout(temp_dir: &TempDir, timeout_secs: u64) -> ContainerConfig {
    let current_dir = std::env::current_dir().unwrap();
    let docker_test_env_path = current_dir.join("integration-tests/docker-test-env");

    ContainerConfig {
        docker_test_env_path,
        project_root: current_dir,
        log_dir: temp_dir.path().join("logs"),
        startup_timeout: std::time::Duration::from_secs(timeout_secs),
        docker_ready_timeout: std::time::Duration::from_secs(timeout_secs),
        ..ContainerConfig::default()
    }
}
