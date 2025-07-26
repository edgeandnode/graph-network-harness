//! Integration tests for service registry event system
//!
//! These tests validate the registry's event subscription and notification
//! system which will be used by the WebSocket API once implemented.

use service_registry::{
    models::{EventType, ServiceState},
    Registry, ServiceEntry,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

mod common;
use common::test_services::*;

/// Test registry with WebSocket-style event handling
#[smol_potat::test]
#[cfg(feature = "integration-tests")]
async fn test_registry_websocket_style_events() {
        let registry = Registry::new().await;
        let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let client2_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8081);

        // Multiple clients subscribe to different events
        registry
            .subscribe(
                client_addr,
                vec![EventType::ServiceRegistered, EventType::ServiceStateChanged],
            )
            .await
            .expect("Failed to subscribe client 1");

        registry
            .subscribe(
                client2_addr,
                vec![EventType::ServiceRegistered, EventType::EndpointUpdated],
            )
            .await
            .expect("Failed to subscribe client 2");

        // Register a service
        let service = create_echo_service().expect("Failed to create service");
        let events = registry
            .register(service)
            .await
            .expect("Failed to register service");

        // Both clients should receive ServiceRegistered event
        assert_eq!(events.len(), 2);
        let addresses: std::collections::HashSet<SocketAddr> =
            events.iter().map(|(addr, _)| *addr).collect();
        assert!(addresses.contains(&client_addr));
        assert!(addresses.contains(&client2_addr));

        // Update state - only client 1 should receive event
        let (_old_state, _events) = registry
            .update_state("echo-service", ServiceState::Starting)
            .await
            .expect("Failed to update state to Starting");
        let (_old_state, events) = registry
            .update_state("echo-service", ServiceState::Running)
            .await
            .expect("Failed to update state to Running");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, client_addr);

        // Update endpoints - only client 2 should receive event
        let endpoint = service_registry::Endpoint::new(
            "http".to_string(),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
            service_registry::Protocol::Http,
        );
        let events = registry
            .update_endpoints("echo-service", vec![endpoint])
            .await
            .expect("Failed to update endpoints");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, client2_addr);
}

/// Test registry subscription management
#[smol_potat::test]
#[cfg(feature = "integration-tests")]
async fn test_registry_subscription_management() {
        let registry = Registry::new().await;
        let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        // Subscribe to multiple events
        registry
            .subscribe(
                client_addr,
                vec![
                    EventType::ServiceRegistered,
                    EventType::ServiceStateChanged,
                    EventType::ServiceDeregistered,
                ],
            )
            .await
            .expect("Failed to subscribe");

        // Register service and verify event
        let service = create_echo_service().expect("Failed to create service");
        let events = registry
            .register(service)
            .await
            .expect("Failed to register service");
        assert_eq!(events.len(), 1);

        // Unsubscribe from some events
        registry
            .unsubscribe(client_addr, vec![EventType::ServiceStateChanged])
            .await
            .expect("Failed to unsubscribe");

        // Update state - should not receive event now since we unsubscribed from ServiceStateChanged
        let (_old_state, events) = registry
            .update_state("echo-service", ServiceState::Starting)
            .await
            .expect("Failed to update state to Starting");
        assert_eq!(events.len(), 0); // Should not receive this event

        let (_old_state, events) = registry
            .update_state("echo-service", ServiceState::Running)
            .await
            .expect("Failed to update state to Running");
        assert_eq!(events.len(), 0); // Should not receive this event either

        // Deregister - should still receive this event
        let (_service, events) = registry
            .deregister("echo-service")
            .await
            .expect("Failed to deregister service");
        assert_eq!(events.len(), 1);

        // Remove subscriber completely
        registry
            .remove_subscriber(client_addr)
            .await
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
        )
        .expect("Failed to create service");

        let events = registry
            .register(service2)
            .await
            .expect("Failed to register service");
        assert_eq!(events.len(), 0);
}

/// Test registry with complex event scenarios
#[smol_potat::test]
#[cfg(feature = "integration-tests")]
async fn test_complex_event_scenarios() {
        let registry = Registry::new().await;

        // Create multiple clients with different subscriptions
        let clients = vec![
            (
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
                vec![EventType::ServiceRegistered],
            ),
            (
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8081),
                vec![EventType::ServiceStateChanged],
            ),
            (
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8082),
                vec![EventType::ServiceRegistered, EventType::ServiceStateChanged],
            ),
        ];

        // Subscribe all clients
        for (addr, events) in &clients {
            registry
                .subscribe(*addr, events.clone())
                .await
                .expect("Failed to subscribe client");
        }

        // Register a service
        let service = create_web_service().expect("Failed to create service");
        let events = registry
            .register(service)
            .await
            .expect("Failed to register service");

        // Clients 0 and 2 should receive ServiceRegistered event
        assert_eq!(events.len(), 2);
        let addresses: std::collections::HashSet<SocketAddr> =
            events.iter().map(|(addr, _)| *addr).collect();
        assert!(addresses.contains(&clients[0].0));
        assert!(addresses.contains(&clients[2].0));
        assert!(!addresses.contains(&clients[1].0));

        // Update state
        let (_old_state, _events) = registry
            .update_state("web-service", ServiceState::Starting)
            .await
            .expect("Failed to update state to Starting");
        let (_old_state, events) = registry
            .update_state("web-service", ServiceState::Running)
            .await
            .expect("Failed to update state to Running");

        // Clients 1 and 2 should receive ServiceStateChanged event
        assert_eq!(events.len(), 2);
        let addresses: std::collections::HashSet<SocketAddr> =
            events.iter().map(|(addr, _)| *addr).collect();
        assert!(addresses.contains(&clients[1].0));
        assert!(addresses.contains(&clients[2].0));
        assert!(!addresses.contains(&clients[0].0));
}
