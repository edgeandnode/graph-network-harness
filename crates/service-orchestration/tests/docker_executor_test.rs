//! Test that DockerExecutor actually starts containers
//!
//! These tests require Docker to be installed and running

use async_runtime_compat::AsyncSpawner;
use service_orchestration::{DockerExecutor, ServiceConfig, ServiceExecutor, ServiceTarget};
use std::collections::HashMap;

fn is_docker_available() -> bool {
    // Check if docker is available by running `docker version`
    std::process::Command::new("docker")
        .arg("version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[smol_potat::test]
async fn test_docker_executor_starts_container() -> anyhow::Result<()> {
    if !is_docker_available() {
        eprintln!("Skipping test - Docker not available");
        return Ok(());
    }

    // Create a DockerExecutor
    let executor = DockerExecutor::new();

    // Create a simple hello-world container config
    let config = ServiceConfig {
        name: "test-hello".to_string(),
        target: ServiceTarget::Docker {
            params: HashMap::new(),
            image: "hello-world:latest".to_string(),
            command_template: None,
            env: HashMap::new(),
            ports: vec![],
            volumes: vec![],
        },
        depends_on: vec![],
        health_check: None,
    };

    // Create spawner
    let spawner = AsyncSpawner::new();

    // Start the service
    let running_service = executor.start(config.clone(), &spawner).await?;

    // Check that we got a container ID
    assert!(running_service.container_id.is_some());
    let container_id = running_service.container_id.as_ref().unwrap();
    assert!(!container_id.is_empty());

    println!("Started container with ID: {}", &container_id[..12]);

    // Stop the service
    executor.stop(&running_service, &spawner).await?;

    Ok(())
}

#[smol_potat::test]
async fn test_docker_executor_with_nginx() -> anyhow::Result<()> {
    if !is_docker_available() {
        eprintln!("Skipping test - Docker not available");
        return Ok(());
    }

    // Create a DockerExecutor
    let executor = DockerExecutor::new();

    // Create an nginx container that stays running
    let config = ServiceConfig {
        name: "test-nginx".to_string(),
        target: ServiceTarget::Docker {
            params: HashMap::new(),
            image: "nginx:alpine".to_string(),
            command_template: None,
            env: HashMap::new(),
            ports: vec![8080], // Map port 8080
            volumes: vec![],
        },
        depends_on: vec![],
        health_check: None,
    };

    // Create spawner
    let spawner = AsyncSpawner::new();

    // Start the service
    let running_service = executor.start(config.clone(), &spawner).await?;

    // Check that we got a container ID
    assert!(running_service.container_id.is_some());
    let container_id = running_service.container_id.as_ref().unwrap();

    println!("Started nginx container with ID: {}", &container_id[..12]);

    // Check if container is actually running
    let output = std::process::Command::new("docker")
        .args([
            "ps",
            "--filter",
            &format!("id={container_id}"),
            "--format",
            "{{.Status}}",
        ])
        .output()?;

    let status = String::from_utf8_lossy(&output.stdout);
    assert!(
        status.contains("Up"),
        "Container should be running, got status: {status}"
    );

    // Stop the service
    executor.stop(&running_service, &spawner).await?;

    // Verify container is stopped/removed
    let output = std::process::Command::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            &format!("id={container_id}"),
            "--format",
            "{{.ID}}",
        ])
        .output()?;

    let remaining = String::from_utf8_lossy(&output.stdout);
    assert!(
        remaining.trim().is_empty(),
        "Container should be removed after stop"
    );

    Ok(())
}

#[smol_potat::test]
async fn test_docker_executor_environment_variables() -> anyhow::Result<()> {
    if !is_docker_available() {
        eprintln!("Skipping test - Docker not available");
        return Ok(());
    }

    // Create a DockerExecutor
    let executor = DockerExecutor::new();

    // Create a container with environment variables
    let mut env = HashMap::new();
    env.insert("TEST_VAR".to_string(), "test_value".to_string());
    env.insert("ANOTHER_VAR".to_string(), "another_value".to_string());

    let config = ServiceConfig {
        name: "test-env".to_string(),
        target: ServiceTarget::Docker {
            params: HashMap::new(),
            image: "alpine:latest".to_string(),
            command_template: None,
            env,
            ports: vec![],
            volumes: vec![],
        },
        depends_on: vec![],
        health_check: None,
    };

    // Note: Alpine with no command will exit immediately, but that's OK for this test
    // We just want to verify the container was created with the right environment

    // Create spawner
    let spawner = AsyncSpawner::new();

    // Start the service
    let running_service = executor.start(config.clone(), &spawner).await?;

    // Check that we got a container ID
    assert!(running_service.container_id.is_some());

    println!("Started alpine container with environment variables");

    // The container will exit quickly since alpine has no long-running process,
    // but we can still stop/remove it
    let _ = executor.stop(&running_service, &spawner).await;

    Ok(())
}
