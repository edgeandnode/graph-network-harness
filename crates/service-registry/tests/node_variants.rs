//! Integration tests for different node execution variants

use command_executor::{Command, Target, target::DockerContainer};
use service_registry::ServiceEntry;
use service_registry::models::{EventType, ServiceState};

mod common;
use common::{test_harness::*, test_services::*};

/// Test local process execution variant
#[smol_potat::test]
#[cfg(feature = "integration-tests")]
async fn test_local_process_variant() {
    let harness = TestHarness::new()
        .await
        .expect("Failed to create test harness");

    // Create and deploy echo service
    let service = create_echo_service().expect("Failed to create echo service");
    let deployment = harness
        .deploy_service(service)
        .await
        .expect("Failed to deploy service");

    // Execute command locally
    let cmd = Command::builder("echo").arg("Local process test").build();
    harness
        .execute_and_track(&deployment.name, cmd, &Target::Command)
        .await
        .expect("Failed to execute command");

    // Wait for service to be running
    harness
        .wait_for_service_state(
            &deployment.name,
            ServiceState::Running,
            std::time::Duration::from_secs(5),
        )
        .await
        .expect("Service did not reach running state");

    // Verify service state
    assert_eq!(deployment.state().await.unwrap(), ServiceState::Running);
}

/// Test Docker container execution variant
#[smol_potat::test]
#[cfg(all(feature = "integration-tests", feature = "docker-tests"))]
async fn test_docker_container_variant() {
    let harness = TestHarness::new()
        .await
        .expect("Failed to create test harness");

    // Create Docker service
    let service = create_docker_service().expect("Failed to create Docker service");
    let deployment = harness
        .deploy_service(service)
        .await
        .expect("Failed to deploy service");

    // Execute via Docker container
    let container = DockerContainer::new("alpine:latest")
        .with_name("test-container")
        .with_remove_on_exit(true);

    let cmd = Command::builder("echo")
        .arg("Docker container test")
        .build();

    match harness
        .execute_and_track(&deployment.name, cmd, &Target::DockerContainer(container))
        .await
    {
        Ok(_) => {
            // Docker execution succeeded
            assert_eq!(deployment.state().await.unwrap(), ServiceState::Running);
        }
        Err(_) => {
            // Docker might not be available in test environment
            println!("Docker not available, marking as failed (expected)");
            assert_eq!(deployment.state().await.unwrap(), ServiceState::Failed);
        }
    }
}

/// Test systemd service execution variant
#[smol_potat::test]
#[cfg(feature = "integration-tests")]
async fn test_systemd_service_variant() {
    let harness = TestHarness::new()
        .await
        .expect("Failed to create test harness");

    // Create systemd service
    let service = create_systemd_service().expect("Failed to create systemd service");
    let deployment = harness
        .deploy_service(service)
        .await
        .expect("Failed to deploy service");

    // Try to check systemd status (will likely fail in test environment)
    let cmd = Command::builder("systemctl")
        .args(["status", "test-daemon.service"])
        .build();

    // This will likely fail, but we test the flow
    let _ = harness
        .execute_and_track(&deployment.name, cmd, &Target::Command)
        .await;

    // In test environment, systemd services will fail
    let final_state = deployment.state().await.unwrap();
    assert!(matches!(
        final_state,
        ServiceState::Failed | ServiceState::Running
    ));
}

/// Test systemd portable execution variant
#[smol_potat::test]
#[cfg(feature = "integration-tests")]
async fn test_systemd_portable_variant() {
    let harness = TestHarness::new()
        .await
        .expect("Failed to create test harness");

    // Create systemd portable service
    let service = create_systemd_portable_service().expect("Failed to create portable service");
    let deployment = harness
        .deploy_service(service)
        .await
        .expect("Failed to deploy service");

    // Try to list portable services
    let cmd = Command::builder("portablectl").arg("list").build();

    // This will likely fail in test environment
    let result = harness
        .execute_and_track(&deployment.name, cmd, &Target::Command)
        .await;

    // In test environment, portable services might succeed or fail
    // depending on whether portablectl is available
    let final_state = deployment.state().await.unwrap();
    if result.is_ok() {
        // Command succeeded - service should be Running
        assert_eq!(final_state, ServiceState::Running);
    } else {
        // Command failed - service should be Failed
        assert_eq!(final_state, ServiceState::Failed);
    }
}

/// Test remote SSH execution variant
#[smol_potat::test]
#[cfg(all(feature = "integration-tests", feature = "ssh-tests"))]
async fn test_remote_ssh_variant() {
    let harness = TestHarness::new()
        .await
        .expect("Failed to create test harness");

    // Create remote service
    let service = create_remote_service().expect("Failed to create remote service");
    let deployment = harness
        .deploy_service(service)
        .await
        .expect("Failed to deploy service");

    // Note: This test registers a remote service but doesn't actually execute via SSH
    // because we don't have SSH launcher integrated in test harness yet
    // This tests the registry functionality with remote location metadata

    let retrieved_service = harness
        .registry
        .get(&deployment.name)
        .await
        .expect("Failed to get remote service");

    // Verify it's marked as remote
    assert!(matches!(
        retrieved_service.location,
        service_registry::Location::Remote { .. }
    ));
}

/// Test multi-node coordination
#[smol_potat::test]
#[cfg(feature = "integration-tests")]
async fn test_multi_node_coordination() {
    let env = MultiNodeTestEnvironment::new(3)
        .await
        .expect("Failed to create multi-node environment");

    // Deploy coordinator to node 0
    let coordinator = create_echo_service().expect("Failed to create coordinator");
    let coord_deployment = env
        .deploy_to_node(0, coordinator)
        .await
        .expect("Failed to deploy coordinator");

    // Deploy workers to nodes 1 and 2
    let worker1 = ServiceEntry::new(
        "worker-1".to_string(),
        "1.0.0".to_string(),
        service_registry::ExecutionInfo::ManagedProcess {
            pid: None,
            command: "echo".to_string(),
            args: vec!["Worker 1".to_string()],
        },
        service_registry::Location::Local,
    )
    .expect("Failed to create worker 1");

    let worker2 = ServiceEntry::new(
        "worker-2".to_string(),
        "1.0.0".to_string(),
        service_registry::ExecutionInfo::ManagedProcess {
            pid: None,
            command: "echo".to_string(),
            args: vec!["Worker 2".to_string()],
        },
        service_registry::Location::Local,
    )
    .expect("Failed to create worker 2");

    let worker1_deployment = env
        .deploy_to_node(1, worker1)
        .await
        .expect("Failed to deploy worker 1");
    let worker2_deployment = env
        .deploy_to_node(2, worker2)
        .await
        .expect("Failed to deploy worker 2");

    // Verify all services are deployed
    let all_services = env
        .list_all_services()
        .await
        .expect("Failed to list all services");

    assert_eq!(all_services.len(), 3); // 3 nodes
    assert_eq!(all_services[0].1.len(), 1); // Node 0: coordinator
    assert_eq!(all_services[1].1.len(), 1); // Node 1: worker-1
    assert_eq!(all_services[2].1.len(), 1); // Node 2: worker-2

    assert_eq!(all_services[0].1[0].name, "echo-service");
    assert_eq!(all_services[1].1[0].name, "worker-1");
    assert_eq!(all_services[2].1[0].name, "worker-2");
}

/// Test complex service dependency graph
#[smol_potat::test]
#[cfg(feature = "integration-tests")]
async fn test_service_dependency_graph() {
    let harness = TestHarness::new()
        .await
        .expect("Failed to create test harness");

    // Create web application stack
    let stack =
        ServiceDependencyGraph::web_application_stack().expect("Failed to create service stack");

    // Deploy services in dependency order
    for service in stack.services {
        let _deployment = harness
            .deploy_service(service)
            .await
            .expect("Failed to deploy service");
    }

    // Verify all services are registered
    let services = harness.registry.list().await;
    assert_eq!(services.len(), 3);

    let service_names: std::collections::HashSet<String> =
        services.iter().map(|s| s.name.clone()).collect();

    assert!(service_names.contains("database"));
    assert!(service_names.contains("backend"));
    assert!(service_names.contains("frontend"));

    // Verify dependencies are set
    let backend = harness
        .registry
        .get("backend")
        .await
        .expect("Failed to get backend service");
    assert!(backend.depends_on.contains(&"database".to_string()));

    let frontend = harness
        .registry
        .get("frontend")
        .await
        .expect("Failed to get frontend service");
    assert!(frontend.depends_on.contains(&"backend".to_string()));
}

/// Test service lifecycle management
#[smol_potat::test]
#[cfg(feature = "integration-tests")]
async fn test_service_lifecycle_management() {
    let harness = TestHarness::new()
        .await
        .expect("Failed to create test harness");

    // Subscribe to events
    let client_addr = std::net::SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
        8080,
    );
    harness
        .registry
        .subscribe(
            client_addr,
            vec![
                EventType::ServiceRegistered,
                EventType::ServiceStateChanged,
                EventType::ServiceDeregistered,
            ],
        )
        .await
        .expect("Failed to subscribe to events");

    // Create and deploy service
    let service = create_web_service().expect("Failed to create web service");
    let deployment = harness
        .deploy_service(service)
        .await
        .expect("Failed to deploy service");

    // Simulate full lifecycle
    let states = vec![
        ServiceState::Starting,
        ServiceState::Running,
        ServiceState::Stopping,
        ServiceState::Stopped,
    ];

    for state in states {
        let (_old_state, events) = harness
            .registry
            .update_state(&deployment.name, state)
            .await
            .expect("Failed to update state");

        // Should generate events for subscribed client
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, client_addr);
    }

    // Remove service
    let (_service, events) = harness
        .registry
        .deregister(&deployment.name)
        .await
        .expect("Failed to deregister service");
    assert_eq!(events.len(), 1);
}

/// Test persistence across restarts
#[smol_potat::test]
#[cfg(feature = "integration-tests")]
async fn test_persistence_across_restarts() {
    let temp_dir = tempfile::TempDir::new().expect("Failed to create temp directory");
    let persist_path = temp_dir.path().join("test-registry.json");

    // First harness instance
    {
        let registry = service_registry::Registry::with_persistence(
            persist_path.to_string_lossy().to_string(),
        )
        .await;

        let service = create_echo_service().expect("Failed to create service");
        registry
            .register(service)
            .await
            .expect("Failed to register service");
        registry
            .update_state("echo-service", ServiceState::Starting)
            .await
            .expect("Failed to update state to Starting");
        registry
            .update_state("echo-service", ServiceState::Running)
            .await
            .expect("Failed to update state to Running");
    } // Registry should persist to file

    // Second harness instance - load from file
    {
        let registry = service_registry::Registry::load(&persist_path)
            .await
            .expect("Failed to load registry");

        let services = registry.list().await;
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].name, "echo-service");
        assert_eq!(services[0].state, ServiceState::Running);
    }
}

/// Test error handling and recovery
#[smol_potat::test]
#[cfg(feature = "integration-tests")]
async fn test_error_handling_recovery() {
    let harness = TestHarness::new()
        .await
        .expect("Failed to create test harness");

    // Create service
    let service = create_echo_service().expect("Failed to create service");
    let deployment = harness
        .deploy_service(service)
        .await
        .expect("Failed to deploy service");

    // Test invalid state transition
    harness
        .registry
        .update_state(&deployment.name, ServiceState::Starting)
        .await
        .expect("Failed to transition to starting");
    harness
        .registry
        .update_state(&deployment.name, ServiceState::Running)
        .await
        .expect("Failed to transition to running");

    let result = harness
        .registry
        .update_state(&deployment.name, ServiceState::Starting)
        .await;
    assert!(result.is_err()); // Should fail - invalid transition

    // Test duplicate registration
    let duplicate_service = create_echo_service().expect("Failed to create duplicate service");
    let result = harness.deploy_service(duplicate_service).await;
    assert!(result.is_err()); // Should fail - service exists

    // Test getting non-existent service
    let result = harness.registry.get("non-existent").await;
    assert!(result.is_err());

    // Test deregistering non-existent service
    let result = harness.registry.deregister("non-existent").await;
    assert!(result.is_err());

    // Verify original service still exists and works
    let service = harness
        .registry
        .get(&deployment.name)
        .await
        .expect("Original service should still exist");
    assert_eq!(service.state, ServiceState::Running);
}
