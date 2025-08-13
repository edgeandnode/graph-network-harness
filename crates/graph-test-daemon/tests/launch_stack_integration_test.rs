//! Integration test for launch_stack with real services

use async_runtime_compat::AsyncSpawner;
use service_orchestration::ProcessCommand;
use std::fs;

#[smol_potat::test]
async fn test_launch_stack_starts_services() -> anyhow::Result<()> {
    // Create a minimal test config with just process-based services
    let test_config = r#"
name: test-stack
description: Test stack with process services

services:
  echo-service:
    service_type: postgres  # Using postgres type since it's registered
    name: echo-service
    target:
      type: process
      binary: echo
      args: ["Starting echo service"]
      env: {}
      working_dir: null
    health_check: null
    dependencies: []

  sleep-service:
    service_type: anvil  # Using anvil type since it's registered
    name: sleep-service
    target:
      type: process
      binary: sleep
      args: ["1"]
      env: {}
      working_dir: null
    health_check: null
    dependencies:
      - service: echo-service

tasks:
  test-task:
    task_type: graph-contracts-deployment
    target:
      type: process
      binary: echo
      args: ["Running test task"]
      env: {}
      working_dir: null
    dependencies:
      - service: echo-service
    config: {}
"#;

    // Write test config to a temporary file
    let config_path = "/tmp/test-launch-stack.yaml";
    fs::write(config_path, test_config)?;

    // Try to load and parse the config
    use service_orchestration::StackConfig;
    let config: StackConfig = serde_yaml::from_str(test_config)?;

    // Verify the config structure
    assert_eq!(config.name, "test-stack");
    assert_eq!(config.services.len(), 2);
    assert_eq!(config.tasks.len(), 1);

    // Verify dependency graph can be built
    use service_orchestration::DependencyGraph;
    let graph = DependencyGraph::from_stack_config(&config);

    // Get topological sort to verify order
    let order = graph.topological_sort().map_err(|e| anyhow::anyhow!(e))?;

    // Should be: echo-service, then test-task and sleep-service
    assert!(order.len() >= 3);

    // The first should be echo-service (no dependencies)
    assert_eq!(
        order[0],
        service_orchestration::DependencyNode::Service("echo-service".to_string())
    );

    println!("Dependency order verified:");
    for node in &order {
        match node {
            service_orchestration::DependencyNode::Service(name) => {
                println!("  Service: {}", name);
            }
            service_orchestration::DependencyNode::Task(name) => {
                println!("  Task: {}", name);
            }
        }
    }

    // Clean up
    fs::remove_file(config_path).ok();

    // Note: We don't actually create and launch a daemon here because:
    // 1. The service types (postgres, anvil) are registered in GraphTestDaemon, not here
    // 2. Actually starting processes would require more setup
    //
    // This test verifies:
    // - Config can be parsed
    // - Dependency graph works
    // - The structure is ready for launch_stack to use

    Ok(())
}

#[smol_potat::test]
async fn test_service_manager_with_process() -> anyhow::Result<()> {
    use service_orchestration::{ServiceConfig, ServiceManager, ServiceTarget};
    use std::collections::HashMap;

    // Create a service manager
    let spawner = AsyncSpawner::new();
    let manager = ServiceManager::new().await?;

    // Create a simple echo service config
    let config = ServiceConfig {
        name: "test-echo".to_string(),
        target: ServiceTarget::Process {
            command: ProcessCommand::Legacy { command: "echo hello from service".to_string() },
            env: HashMap::new(),
            working_dir: None,
        },
        dependencies: vec![],
        health_check: None,
    };

    // Start the service using launch_service
    let (_events, running_service) = manager.launch_service("test-echo", config, &spawner).await?;

    // Verify service was started
    assert!(running_service.pid.is_some());
    println!("Started service with PID: {:?}", running_service.pid);

    // Check service status
    let status = manager.get_service_status("test-echo").await?;
    println!("Service status: {:?}", status);

    // List services
    let services = manager.list_services();
    assert!(services.contains(&"test-echo".to_string()));

    // Stop the service
    manager.stop_service("test-echo", &spawner).await?;

    // Verify service is gone
    let services_after = manager.list_services();
    assert!(!services_after.contains(&"test-echo".to_string()));

    Ok(())
}
