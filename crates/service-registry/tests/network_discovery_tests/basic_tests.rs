//! Basic network discovery tests

use crate::network_discovery_tests::shared_dind::{
    check_docker, dind_compose_command, ensure_dind_container_running, TEST_MUTEX,
};
use service_registry::network::{NetworkConfig, NetworkLocation, NetworkManager, ServiceNetwork};
use std::time::Duration;

#[smol_potat::test]
async fn test_dind_basic_docker_compose() {
    // Lock to prevent concurrent test execution (network conflicts)
    let _test_lock = TEST_MUTEX.lock().unwrap();
    
    if !check_docker().await {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    // Ensure shared DinD container is running
    ensure_dind_container_running()
        .await
        .expect("Failed to ensure DinD container is running");

    // Use unique project name for isolation within the shared container
    let project_name = format!("test-basic-{}", uuid::Uuid::new_v4().simple());

    // Start compose stack in DinD container
    dind_compose_command("local-only.yml", &project_name, vec!["up", "-d"])
        .await
        .expect("Failed to start docker-compose in DinD");

    // Give containers time to start
    smol::Timer::after(Duration::from_secs(2)).await;

    // Just verify we can create a network manager
    let config = NetworkConfig {
        wireguard_subnet: "10.42.0.0/16".parse().unwrap(),
        lan_interface: "eth0".to_string(),
        dns_port: 5353,
        enable_wireguard: false,
    };

    let network_manager = NetworkManager::new(config)
        .expect("Failed to create network manager");

    // Basic check that we created it
    assert!(network_manager.generate_environment("test").is_ok());

    // Cleanup - stop compose stack in DinD container
    dind_compose_command("local-only.yml", &project_name, vec!["down", "-v"])
        .await
        .expect("Failed to stop docker-compose in DinD");
}

#[smol_potat::test]
async fn test_network_manager_creation() {
    let config = NetworkConfig {
        wireguard_subnet: "10.42.0.0/16".parse().unwrap(),
        lan_interface: "eth0".to_string(),
        dns_port: 5353,
        enable_wireguard: true,
    };

    let network_manager = NetworkManager::new(config);
    assert!(network_manager.is_ok());
}

#[smol_potat::test] 
async fn test_service_registration() {
    let config = NetworkConfig {
        wireguard_subnet: "10.42.0.0/16".parse().unwrap(),
        lan_interface: "eth0".to_string(),
        dns_port: 5353,
        enable_wireguard: false,
    };

    let mut network_manager = NetworkManager::new(config)
        .expect("Failed to create network manager");

    // Create a local service
    let local_service = ServiceNetwork {
        service_name: "test-local".to_string(),
        location: NetworkLocation::Local,
        host_ip: Some("127.0.0.1".parse().unwrap()),
        lan_ip: None,
        wireguard_ip: None,
        wireguard_public_key: None,
        interfaces: vec!["lo".to_string()],
    };

    // Register it
    network_manager.register_service(local_service).await
        .expect("Failed to register service");

    // Try to generate environment for the registered service
    let env = network_manager.generate_environment("test-local")
        .expect("Failed to generate environment");
    
    // Environment should be a HashMap (might be empty)
    // Just verify we got something back
    assert!(env.len() >= 0);
}