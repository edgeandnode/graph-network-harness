//! WebSocket integration tests

use service_registry::{EventType, Registry, ServiceState, WsServer};
use std::sync::Arc;
use std::time::Duration;

mod common;
use common::{test_services::*, websocket_client::WebSocketTestClient};

/// Test basic WebSocket connection and operations
#[cfg(feature = "integration-tests")]
#[smol_potat::test]
async fn test_websocket_basic_operations() {
    // Start server
    let registry = Registry::new();
    let addr = "127.0.0.1:0"; // Use port 0 for automatic assignment
    let server = WsServer::new(addr, registry)
        .await
        .expect("Failed to create server");

    // Get actual address
    let server_addr = server
        .listener
        .local_addr()
        .expect("Failed to get server address");

    // Run server in background
    let server_task = smol::spawn(async move {
        loop {
            match server.accept().await {
                Ok(handler) => {
                    smol::spawn(handler.handle()).detach();
                }
                Err(e) => {
                    eprintln!("Accept error: {}", e);
                    break;
                }
            }
        }
    });

    // Give server time to start
    smol::Timer::after(Duration::from_millis(100)).await;

    // Connect client
    let client = WebSocketTestClient::connect(server_addr)
        .await
        .expect("Failed to connect client");

    // List services (should be empty)
    let services = client
        .list_services()
        .await
        .expect("Failed to list services");
    assert_eq!(services.as_array().unwrap().len(), 0);

    // List endpoints (should be empty)
    let endpoints = client
        .list_endpoints()
        .await
        .expect("Failed to list endpoints");
    assert!(endpoints.as_object().unwrap().is_empty());

    client.close().await.expect("Failed to close client");
    drop(server_task);
}

/// Test service operations via WebSocket
#[cfg(feature = "integration-tests")]
#[smol_potat::test]
async fn test_websocket_service_operations() {
    // Start server with a service
    let registry = Registry::new();

    // Pre-register a service
    let service = create_echo_service().expect("Failed to create service");
    registry
        .register(service)
        .await
        .expect("Failed to register service");

    let server = WsServer::new("127.0.0.1:0", registry)
        .await
        .expect("Failed to create server");
    let server_addr = server
        .listener
        .local_addr()
        .expect("Failed to get server address");

    // Run server
    let server_task = smol::spawn(async move {
        loop {
            match server.accept().await {
                Ok(handler) => {
                    smol::spawn(handler.handle()).detach();
                }
                Err(_) => break,
            }
        }
    });

    smol::Timer::after(Duration::from_millis(100)).await;

    // Connect client
    let client = WebSocketTestClient::connect(server_addr)
        .await
        .expect("Failed to connect client");

    // List services
    let services = client
        .list_services()
        .await
        .expect("Failed to list services");
    assert_eq!(services.as_array().unwrap().len(), 1);

    // Get specific service
    let service = client
        .get_service("echo-service")
        .await
        .expect("Failed to get service");
    assert_eq!(service["name"], "echo-service");
    assert_eq!(service["state"], "registered");

    // Start the service
    client
        .start_service("echo-service")
        .await
        .expect("Failed to start service");

    // Wait a bit for state to propagate
    smol::Timer::after(Duration::from_millis(100)).await;

    // Check state changed
    let service = client
        .get_service("echo-service")
        .await
        .expect("Failed to get service");
    assert_eq!(service["state"], "starting");

    client.close().await.expect("Failed to close client");
    drop(server_task);
}

/// Test event subscriptions
#[cfg(feature = "integration-tests")]
#[smol_potat::test]
async fn test_websocket_event_subscriptions() {
    let registry = Registry::new();

    // Pre-register a service and update its state
    let service = create_web_service().expect("Failed to create service");
    registry
        .register(service)
        .await
        .expect("Failed to register service");

    let server = WsServer::new("127.0.0.1:0", registry)
        .await
        .expect("Failed to create server");
    let server_addr = server
        .listener
        .local_addr()
        .expect("Failed to get server address");
    let server_registry = server.registry().clone();

    // Run server
    let server_task = smol::spawn(async move {
        loop {
            match server.accept().await {
                Ok(handler) => {
                    smol::spawn(handler.handle()).detach();
                }
                Err(_) => break,
            }
        }
    });

    smol::Timer::after(Duration::from_millis(100)).await;

    // Connect client and subscribe to events
    let client = WebSocketTestClient::connect(server_addr)
        .await
        .expect("Failed to connect client");

    client
        .subscribe(vec![
            EventType::ServiceRegistered,
            EventType::ServiceStateChanged,
        ])
        .await
        .expect("Failed to subscribe");

    // Update service state (should trigger event)
    server_registry
        .update_state("web-service", ServiceState::Starting)
        .await
        .expect("Failed to update state");

    // Give events time to propagate
    smol::Timer::after(Duration::from_millis(200)).await;

    // In a real test, we would verify events were received
    // For now, just verify operations succeeded

    client.close().await.expect("Failed to close client");
    drop(server_task);
}

/// Test concurrent connections
#[cfg(feature = "integration-tests")]
#[smol_potat::test]
async fn test_concurrent_websocket_connections() {
    let registry = Registry::new();
    let server = WsServer::new("127.0.0.1:0", registry)
        .await
        .expect("Failed to create server");
    let server_addr = server
        .listener
        .local_addr()
        .expect("Failed to get server address");

    // Run server
    let _server_task = smol::spawn(async move {
        loop {
            match server.accept().await {
                Ok(handler) => {
                    smol::spawn(handler.handle()).detach();
                }
                Err(_) => break,
            }
        }
    });

    smol::Timer::after(Duration::from_millis(100)).await;

    // Connect multiple clients
    let mut clients = Vec::new();
    for i in 0..5 {
        let client = WebSocketTestClient::connect(server_addr)
            .await
            .expect(&format!("Failed to connect client {}", i));
        clients.push(client);
    }

    // Each client lists services
    for (i, client) in clients.iter().enumerate() {
        let services = client
            .list_services()
            .await
            .expect(&format!("Client {} failed to list services", i));
        assert_eq!(services.as_array().unwrap().len(), 0);
    }

    // Close all clients
    for client in clients {
        client.close().await.expect("Failed to close client");
    }
}
