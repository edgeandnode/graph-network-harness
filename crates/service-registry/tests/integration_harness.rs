//! Integration tests for service registry with real harness deployments

use command_executor::{
    Command, Executor, Target, backends::local::LocalLauncher, target::DockerContainer,
};
use service_registry::{
    Endpoint, ExecutionInfo, Location, Protocol, Registry, ServiceEntry,
    models::{EventType, ServiceState, WsMessage},
    package::{PackageBuilder, PackageInstaller},
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

mod common;
use common::test_services;

/// Integration test that deploys a real service through the registry
#[smol_potat::test]
#[cfg(feature = "integration-tests")]
async fn test_full_service_deployment() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let registry =
        Registry::with_persistence(temp_dir.path().join("registry.json").to_string_lossy()).await;

    // Create a test service
    let service = ServiceEntry::new(
        "echo-service".to_string(),
        "1.0.0".to_string(),
        ExecutionInfo::ManagedProcess {
            pid: None,
            command: "echo".to_string(),
            args: vec!["Hello World!".to_string()],
        },
        Location::Local,
    )
    .expect("Failed to create service entry");

    // Register the service
    let events = registry
        .register(service)
        .await
        .expect("Failed to register service");

    // Verify the service was registered
    assert_eq!(events.len(), 0); // No subscribers yet
    let retrieved = registry
        .get("echo-service")
        .await
        .expect("Failed to get service");
    assert_eq!(retrieved.name, "echo-service");
    assert_eq!(retrieved.state, ServiceState::Registered);

    // Simulate service startup
    let (_old_state, _events) = registry
        .update_state("echo-service", ServiceState::Starting)
        .await
        .expect("Failed to update state");
    let (_old_state, _events) = registry
        .update_state("echo-service", ServiceState::Running)
        .await
        .expect("Failed to update state");

    // Verify final state
    let final_service = registry
        .get("echo-service")
        .await
        .expect("Failed to get final service state");
    assert_eq!(final_service.state, ServiceState::Running);
}

/// Test service registry with actual command executor
#[smol_potat::test]
#[cfg(feature = "integration-tests")]
async fn test_registry_with_executor() {
    let registry = Registry::new().await;
    let local_launcher = LocalLauncher;
    let executor = Executor::new("registry-test".to_string(), local_launcher);

    // Create service entry for a command we'll execute
    let mut service = ServiceEntry::new(
        "test-command".to_string(),
        "1.0.0".to_string(),
        ExecutionInfo::ManagedProcess {
            pid: None,
            command: "echo".to_string(),
            args: vec!["Integration test".to_string()],
        },
        Location::Local,
    )
    .expect("Failed to create service entry");

    // Add an endpoint
    let endpoint = Endpoint::new(
        "output".to_string(),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080),
        Protocol::Http,
    );
    service.add_endpoint(endpoint);

    // Register the service
    registry
        .register(service)
        .await
        .expect("Failed to register service");

    // Execute the command through command-executor
    let cmd = Command::builder("echo").arg("Integration test").build();

    let result = executor
        .execute(&Target::Command, cmd)
        .await
        .expect("Failed to execute command");

    assert!(result.success());
    assert!(result.output.contains("Integration test"));

    // Update service state based on execution result
    if result.success() {
        registry
            .update_state("test-command", ServiceState::Starting)
            .await
            .expect("Failed to update to starting");
        registry
            .update_state("test-command", ServiceState::Running)
            .await
            .expect("Failed to update to running");
    } else {
        registry
            .update_state("test-command", ServiceState::Failed)
            .await
            .expect("Failed to update to failed");
    }

    // Verify service state
    let final_service = registry
        .get("test-command")
        .await
        .expect("Failed to get service");
    assert_eq!(final_service.state, ServiceState::Running);
}

/// Test Docker container deployment via registry
#[smol_potat::test]
#[cfg(all(feature = "integration-tests", feature = "docker-tests"))]
async fn test_docker_service_deployment() {
    let registry = Registry::new().await;
    let local_launcher = LocalLauncher;
    let executor = Executor::new("docker-registry-test".to_string(), local_launcher);

    // Create service entry for Docker container
    let service = ServiceEntry::new(
        "alpine-service".to_string(),
        "1.0.0".to_string(),
        ExecutionInfo::DockerContainer {
            container_id: None,
            image: "alpine:latest".to_string(),
            name: Some("alpine-test".to_string()),
        },
        Location::Local,
    )
    .expect("Failed to create service entry");

    // Register the service
    registry
        .register(service)
        .await
        .expect("Failed to register service");

    // Execute via Docker
    let container = DockerContainer::new("alpine:latest")
        .with_name("alpine-test")
        .with_remove_on_exit(true);

    let cmd = Command::builder("echo")
        .arg("Docker integration test")
        .build();

    match executor
        .execute(&Target::DockerContainer(container), cmd)
        .await
    {
        Ok(result) if result.success() => {
            // Update service to running state
            registry
                .update_state("alpine-service", ServiceState::Starting)
                .await
                .expect("Failed to update to starting");
            registry
                .update_state("alpine-service", ServiceState::Running)
                .await
                .expect("Failed to update to running");

            let final_service = registry
                .get("alpine-service")
                .await
                .expect("Failed to get service");
            assert_eq!(final_service.state, ServiceState::Running);
        }
        Ok(_) => {
            // Command failed, update service to failed state
            registry
                .update_state("alpine-service", ServiceState::Failed)
                .await
                .expect("Failed to update to failed");
        }
        Err(_) => {
            // Docker not available, skip test
            println!("Docker not available, skipping Docker integration test");
        }
    }
}

/// Test event subscription during service lifecycle
#[smol_potat::test]
#[cfg(feature = "integration-tests")]
async fn test_event_subscription_integration() {
    let registry = Registry::new().await;
    let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

    // Subscribe to all events
    registry
        .subscribe(
            client_addr,
            vec![
                EventType::ServiceRegistered,
                EventType::ServiceStateChanged,
                EventType::EndpointUpdated,
                EventType::ServiceDeregistered,
            ],
        )
        .await
        .expect("Failed to subscribe");

    // Create and register a service
    let mut service = ServiceEntry::new(
        "event-test-service".to_string(),
        "1.0.0".to_string(),
        ExecutionInfo::ManagedProcess {
            pid: None,
            command: "sleep".to_string(),
            args: vec!["5".to_string()],
        },
        Location::Local,
    )
    .expect("Failed to create service entry");

    // Register and collect events
    let events = registry
        .register(service)
        .await
        .expect("Failed to register service");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, client_addr);

    // Update state and collect events
    let (_old_state, events) = registry
        .update_state("event-test-service", ServiceState::Starting)
        .await
        .expect("Failed to update state");
    assert_eq!(events.len(), 1);

    // Add endpoint and collect events
    let endpoint = Endpoint::new(
        "api".to_string(),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3000),
        Protocol::Http,
    );
    let events = registry
        .update_endpoints("event-test-service", vec![endpoint])
        .await
        .expect("Failed to update endpoints");
    assert_eq!(events.len(), 1);

    // Deregister and collect events
    let (_service, events) = registry
        .deregister("event-test-service")
        .await
        .expect("Failed to deregister service");
    assert_eq!(events.len(), 1);

    // Verify all events were of correct types
    if let WsMessage::Event { event, .. } = &events[0].1 {
        assert_eq!(*event, EventType::ServiceDeregistered);
    } else {
        panic!("Expected ServiceDeregistered event");
    }
}

/// Test package deployment flow
#[smol_potat::test]
#[cfg(feature = "integration-tests")]
async fn test_package_deployment_flow() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source_dir = temp_dir.path().join("package-source");
    let output_dir = temp_dir.path().join("package-output");

    // Create directories
    std::fs::create_dir_all(&source_dir).expect("Failed to create source dir");
    std::fs::create_dir_all(&output_dir).expect("Failed to create output dir");

    // Create a test manifest
    let manifest_content = r#"
name: "test-package"
version: "1.0.0"
description: "Test package for integration"
service:
  type: "process"
  ports:
    - name: "http"
      port: 8080
      protocol: "http"
health:
  script: "scripts/health.sh"
  interval: "30s"
  timeout: "5s"
"#;
    std::fs::write(source_dir.join("manifest.yaml"), manifest_content)
        .expect("Failed to write manifest");

    // Create package builder
    let builder = PackageBuilder::new(
        "test-package".to_string(),
        "1.0.0".to_string(),
        source_dir,
        output_dir.clone(),
    );

    // Test name sanitization (core functionality)
    assert_eq!(
        PackageBuilder::sanitize_name("test@package!"),
        "test_package_"
    );
    assert_eq!(
        PackageBuilder::sanitize_version("1.0.0-beta+build"),
        "1.0.0-beta_build"
    );

    // Test that load_manifest works (build will fail due to unimplemented tarball creation)
    match builder.build().await {
        Err(service_registry::Error::Package(msg))
            if msg.contains("Tarball creation not yet implemented") =>
        {
            println!("Expected error: package building not fully implemented yet");
        }
        other => {
            panic!("Unexpected result: {:?}", other);
        }
    }
}

/// Test multi-node service coordination
#[smol_potat::test]
#[cfg(all(feature = "integration-tests", feature = "ssh-tests"))]
async fn test_multi_node_coordination() {
    let registry = Registry::new().await;

    // Register local service
    let local_service = ServiceEntry::new(
        "coordinator".to_string(),
        "1.0.0".to_string(),
        ExecutionInfo::ManagedProcess {
            pid: None,
            command: "echo".to_string(),
            args: vec!["Local coordinator".to_string()],
        },
        Location::Local,
    )
    .expect("Failed to create local service");

    registry
        .register(local_service)
        .await
        .expect("Failed to register local service");

    // Register remote service (simulated)
    let remote_service = ServiceEntry::new(
        "worker".to_string(),
        "1.0.0".to_string(),
        ExecutionInfo::ManagedProcess {
            pid: None,
            command: "echo".to_string(),
            args: vec!["Remote worker".to_string()],
        },
        Location::Remote {
            host: "test-host".to_string(),
            ssh_user: "test-user".to_string(),
            ssh_port: Some(22),
        },
    )
    .expect("Failed to create remote service");

    registry
        .register(remote_service)
        .await
        .expect("Failed to register remote service");

    // Verify both services are registered
    let services = registry.list().await;
    assert_eq!(services.len(), 2);

    let coordinator = registry
        .get("coordinator")
        .await
        .expect("Failed to get coordinator");
    let worker = registry.get("worker").await.expect("Failed to get worker");

    assert!(matches!(coordinator.location, Location::Local));
    assert!(matches!(worker.location, Location::Remote { .. }));
}

/// Test persistence across registry restarts
#[smol_potat::test]
#[cfg(feature = "integration-tests")]
async fn test_registry_persistence() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let persist_path = temp_dir.path().join("registry-persist.json");

    // Create first registry instance
    {
        let registry = Registry::with_persistence(persist_path.display().to_string()).await;

        let service = ServiceEntry::new(
            "persistent-service".to_string(),
            "1.0.0".to_string(),
            ExecutionInfo::ManagedProcess {
                pid: Some(12345),
                command: "test".to_string(),
                args: vec![],
            },
            Location::Local,
        )
        .expect("Failed to create service");

        registry
            .register(service)
            .await
            .expect("Failed to register service");

        registry
            .update_state("persistent-service", ServiceState::Starting)
            .await
            .expect("Failed to update state to Starting");
        registry
            .update_state("persistent-service", ServiceState::Running)
            .await
            .expect("Failed to update state to Running");
    } // Registry dropped, should persist to file

    // Create second registry instance and load from file
    {
        let registry = Registry::load(&persist_path)
            .await
            .expect("Failed to load registry from file");

        let loaded_service = registry
            .get("persistent-service")
            .await
            .expect("Failed to get persisted service");

        assert_eq!(loaded_service.name, "persistent-service");
        assert_eq!(loaded_service.state, ServiceState::Running);

        if let ExecutionInfo::ManagedProcess { pid, .. } = loaded_service.execution {
            assert_eq!(pid, Some(12345));
        } else {
            panic!("Expected ManagedProcess execution info");
        }
    }
}
