//! WebSocket integration tests for service registry

use service_registry::{
    Registry, ServiceEntry,
    models::{Action, EventType, ServiceState},
};
use std::net::{SocketAddr, IpAddr, Ipv4Addr};
use std::time::Duration;

mod common;
use common::{
    test_services::*,
    websocket_client::WebSocketTestClient,
    integration_test,
};

// NOTE: These tests require a WebSocket server implementation
// They are currently structured to test the client-side functionality
// and will be enabled once the WebSocket server is implemented

/// Test WebSocket client connection and basic operations
#[test]
#[cfg(all(feature = "integration-tests", feature = "websocket-server"))]
fn test_websocket_basic_operations() {
    smol::block_on(async {
        // This test would require a running WebSocket server
        let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        
        // Connect to WebSocket server
        let mut client = WebSocketTestClient::connect(server_addr).await
            .expect("Failed to connect to WebSocket server");
        
        // List services (should be empty initially)
        let services = client.list_services().await
            .expect("Failed to list services");
        
        assert_eq!(services.as_array().unwrap().len(), 0);
        
        // List endpoints (should be empty initially)
        let endpoints = client.list_endpoints().await
            .expect("Failed to list endpoints");
        
        assert!(endpoints.as_object().unwrap().is_empty());
        
        client.close().await.expect("Failed to close connection");
    });
}

/// Test WebSocket event subscription
#[test]
#[cfg(all(feature = "integration-tests", feature = "websocket-server"))]
fn test_websocket_event_subscription() {
    smol::block_on(async {
        let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        
        let mut client = WebSocketTestClient::connect(server_addr).await
            .expect("Failed to connect to WebSocket server");
        
        // Subscribe to events
        client.subscribe(vec![
            EventType::ServiceRegistered,
            EventType::ServiceStateChanged,
        ]).await.expect("Failed to subscribe to events");
        
        // In a real test, we would trigger service registration here
        // and wait for events to be received
        
        client.close().await.expect("Failed to close connection");
    });
}

/// Test WebSocket package deployment
#[test]
#[cfg(all(feature = "integration-tests", feature = "websocket-server"))]
fn test_websocket_package_deployment() {
    smol::block_on(async {
        let temp_dir = tempfile::TempDir::new().expect("Failed to create temp directory");
        let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        
        // Create test package
        let package_dir = create_test_package(&temp_dir, "test-service", "1.0.0").await
            .expect("Failed to create test package");
        
        let mut client = WebSocketTestClient::connect(server_addr).await
            .expect("Failed to connect to WebSocket server");
        
        // Deploy package
        let result = client.deploy_package(
            package_dir.to_str().unwrap(),
            Some("local-node")
        ).await.expect("Failed to deploy package");
        
        // Verify deployment result
        assert!(result.get("success").unwrap().as_bool().unwrap());
        
        client.close().await.expect("Failed to close connection");
    });
}

/// Test concurrent WebSocket connections
#[test]
#[cfg(all(feature = "integration-tests", feature = "websocket-server"))]
fn test_concurrent_websocket_connections() {
    smol::block_on(async {
        let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        
        // Create multiple concurrent connections
        let mut clients = Vec::new();
        for i in 0..5 {
            let client = WebSocketTestClient::connect(server_addr).await
                .expect(&format!("Failed to connect client {}", i));
            clients.push(client);
        }
        
        // Each client subscribes to events
        for (i, client) in clients.iter_mut().enumerate() {
            client.subscribe(vec![EventType::ServiceRegistered]).await
                .expect(&format!("Failed to subscribe client {}", i));
        }
        
        // In a real test, we would register a service and verify
        // all clients receive the event
        
        // Close all connections
        for client in clients {
            client.close().await.expect("Failed to close connection");
        }
    });
}

// The following tests use direct registry integration to test
// WebSocket-like functionality without requiring a running server

/// Test registry with WebSocket-style event handling
integration_test!(
    test_registry_websocket_style_events,
    features = ["integration-tests"],
    {
        let registry = Registry::new();
        let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let client2_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8081);
        
        // Multiple clients subscribe to different events
        registry.subscribe(client_addr, vec![
            EventType::ServiceRegistered,
            EventType::ServiceStateChanged,
        ]).await.expect("Failed to subscribe client 1");
        
        registry.subscribe(client2_addr, vec![
            EventType::ServiceRegistered,
            EventType::EndpointUpdated,
        ]).await.expect("Failed to subscribe client 2");
        
        // Register a service
        let service = create_echo_service().expect("Failed to create service");
        let events = registry.register(service).await.expect("Failed to register service");
        
        // Both clients should receive ServiceRegistered event
        assert_eq!(events.len(), 2);
        let addresses: std::collections::HashSet<SocketAddr> = events.iter()
            .map(|(addr, _)| *addr)
            .collect();
        assert!(addresses.contains(&client_addr));
        assert!(addresses.contains(&client2_addr));
        
        // Update state - only client 1 should receive event
        let (_old_state, events) = registry.update_state("echo-service", ServiceState::Starting).await
            .expect("Failed to update state to Starting");
        let (_old_state, events) = registry.update_state("echo-service", ServiceState::Running).await
            .expect("Failed to update state to Running");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, client_addr);
        
        // Update endpoints - only client 2 should receive event  
        let endpoint = service_registry::Endpoint::new(
            "http".to_string(),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
            service_registry::Protocol::Http,
        );
        let events = registry.update_endpoints("echo-service", vec![endpoint]).await
            .expect("Failed to update endpoints");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, client2_addr);
    }
);

/// Test registry subscription management
integration_test!(
    test_registry_subscription_management,
    features = ["integration-tests"],
    {
        let registry = Registry::new();
        let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        
        // Subscribe to multiple events
        registry.subscribe(client_addr, vec![
            EventType::ServiceRegistered,
            EventType::ServiceStateChanged,
            EventType::ServiceDeregistered,
        ]).await.expect("Failed to subscribe");
        
        // Register service and verify event
        let service = create_echo_service().expect("Failed to create service");
        let events = registry.register(service).await.expect("Failed to register service");
        assert_eq!(events.len(), 1);
        
        // Unsubscribe from some events
        registry.unsubscribe(client_addr, vec![EventType::ServiceStateChanged]).await
            .expect("Failed to unsubscribe");
        
        // Update state - should not receive event now since we unsubscribed from ServiceStateChanged
        let (_old_state, events) = registry.update_state("echo-service", ServiceState::Starting).await
            .expect("Failed to update state to Starting");
        assert_eq!(events.len(), 0); // Should not receive this event
        
        let (_old_state, events) = registry.update_state("echo-service", ServiceState::Running).await
            .expect("Failed to update state to Running");
        assert_eq!(events.len(), 0); // Should not receive this event either
        
        // Deregister - should still receive this event
        let (_service, events) = registry.deregister("echo-service").await
            .expect("Failed to deregister service");
        assert_eq!(events.len(), 1);
        
        // Remove subscriber completely
        registry.remove_subscriber(client_addr).await
            .expect("Failed to remove subscriber");
        
        // Register another service - should not receive event
        let service2 = ServiceEntry::new(
            "test-service-2".to_string(),
            "1.0.0".to_string(),
            service_registry::ExecutionInfo::ManagedProcess {
                pid: None,
                command: "test".to_string(),
                args: vec![],
            },
            service_registry::Location::Local,
        ).expect("Failed to create service");
        
        let events = registry.register(service2).await.expect("Failed to register service");
        assert_eq!(events.len(), 0);
    }
);

/// Test registry with complex event scenarios
integration_test!(
    test_complex_event_scenarios,
    features = ["integration-tests"],
    {
        let registry = Registry::new();
        
        // Create multiple clients with different subscriptions
        let clients = vec![
            (SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080), 
             vec![EventType::ServiceRegistered]),
            (SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8081), 
             vec![EventType::ServiceStateChanged]),
            (SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8082), 
             vec![EventType::ServiceRegistered, EventType::ServiceStateChanged]),
        ];
        
        // Subscribe all clients
        for (addr, events) in &clients {
            registry.subscribe(*addr, events.clone()).await
                .expect("Failed to subscribe client");
        }
        
        // Register a service
        let service = create_web_service().expect("Failed to create service");
        let events = registry.register(service).await.expect("Failed to register service");
        
        // Clients 0 and 2 should receive ServiceRegistered event
        assert_eq!(events.len(), 2);
        let addresses: std::collections::HashSet<SocketAddr> = events.iter()
            .map(|(addr, _)| *addr)
            .collect();
        assert!(addresses.contains(&clients[0].0));
        assert!(addresses.contains(&clients[2].0));
        assert!(!addresses.contains(&clients[1].0));
        
        // Update state
        let (_old_state, events) = registry.update_state("web-service", ServiceState::Starting).await
            .expect("Failed to update state to Starting");
        let (_old_state, events) = registry.update_state("web-service", ServiceState::Running).await
            .expect("Failed to update state to Running");
        
        // Clients 1 and 2 should receive ServiceStateChanged event
        assert_eq!(events.len(), 2);
        let addresses: std::collections::HashSet<SocketAddr> = events.iter()
            .map(|(addr, _)| *addr)
            .collect();
        assert!(addresses.contains(&clients[1].0));
        assert!(addresses.contains(&clients[2].0));
        assert!(!addresses.contains(&clients[0].0));
    }
);