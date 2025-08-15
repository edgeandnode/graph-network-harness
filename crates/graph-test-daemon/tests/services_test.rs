//! Tests for the new service implementations

use graph_test_daemon::services::*;
use harness_core::service::{JsonServiceRegistry, Service};

#[smol_potat::test]
#[ignore = "Service actions not yet implemented (uses todo!())"]
async fn test_graph_node_service() {
    let service = GraphNodeService::new("localhost".to_string());

    // Test service metadata
    assert_eq!(service.name(), "graph-node");
    assert!(service.description().contains("Graph Protocol"));

    // Test deploy subgraph action would panic with todo!()
    // Re-enable this test once the action handler is implemented
}

#[smol_potat::test]
#[ignore = "Service actions not yet implemented (uses todo!())"]
async fn test_anvil_service() {
    let service = AnvilService::new(31337, 8545);

    // Test service metadata
    assert_eq!(service.name(), "anvil");
    assert!(service.description().contains("Anvil"));

    // Test mine blocks action would panic with todo!()
    // Re-enable this test once the action handler is implemented
}

#[test]
fn test_service_metadata() {
    // Test that services can be created and have correct metadata
    let graph_node = GraphNodeService::new("localhost".to_string());
    assert_eq!(graph_node.name(), "graph-node");
    assert_eq!(GraphNodeService::SERVICE_TYPE, "graph-node");

    let anvil = AnvilService::new(31337, 8545);
    assert_eq!(anvil.name(), "anvil");
    assert_eq!(AnvilService::SERVICE_TYPE, "anvil");

    let postgres = PostgresService::new("graph-node".to_string(), 5432);
    assert_eq!(postgres.name(), "postgres");
    assert_eq!(PostgresService::SERVICE_TYPE, "postgres");

    let ipfs = IpfsService::new(5001, 8080);
    assert_eq!(ipfs.name(), "ipfs");
    assert_eq!(IpfsService::SERVICE_TYPE, "ipfs");
}

#[test]
fn test_json_service_registry_registration() {
    let mut stack = JsonServiceRegistry::new();

    // Register Graph Node
    let graph_node = GraphNodeService::new("localhost".to_string());
    stack
        .register("graph-node-1".to_string(), graph_node)
        .unwrap();

    // Register Anvil
    let anvil = AnvilService::new(31337, 8545);
    stack.register("anvil-1".to_string(), anvil).unwrap();

    // Check registration
    assert_eq!(stack.list().len(), 2);
    assert!(stack.get("graph-node-1").is_some());
    assert!(stack.get("anvil-1").is_some());

    // Check all actions
    let actions = stack.all_actions();
    assert!(!actions.is_empty());
}

#[test]
fn test_json_schema_generation() {
    // The #[json_actions] macro generates action types from the service methods
    // We can test that events can generate JSON schemas
    use graph_test_daemon::services::GraphNodeEvent;

    let event_schema = schemars::schema_for!(GraphNodeEvent);

    // Convert to JSON to verify they're valid
    let event_json = serde_json::to_value(event_schema).unwrap();

    // Basic validation - schemas should have a schema field
    assert!(event_json.get("$schema").is_some());
}

#[test]
fn test_complete_graph_stack_registration() {
    let mut stack = JsonServiceRegistry::new();

    // Register all Graph Protocol services
    stack
        .register(
            "graph-node-1".to_string(),
            GraphNodeService::new("localhost".to_string()),
        )
        .unwrap();
    stack
        .register("anvil-1".to_string(), AnvilService::new(31337, 8545))
        .unwrap();
    stack
        .register(
            "postgres-1".to_string(),
            PostgresService::new("graph-node".to_string(), 5432),
        )
        .unwrap();
    stack
        .register("ipfs-1".to_string(), IpfsService::new(5001, 8080))
        .unwrap();

    // Check all services are registered
    assert_eq!(stack.list().len(), 4);

    // Test action discovery
    let all_actions = stack.all_actions();
    assert!(all_actions.len() >= 4); // At least one action per service
}

#[smol_potat::test]
#[ignore = "Service actions not yet implemented (uses todo!())"]
async fn test_complete_graph_stack_dispatch() {
    // This test would dispatch actions to services
    // Currently disabled because action handlers use todo!()
    // Re-enable once implementations are added
}
