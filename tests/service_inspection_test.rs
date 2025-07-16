//! Integration tests for service inspection functionality

use local_network_harness::inspection::{
    ServiceEventHandler, GenericEventHandler, PostgresEventHandler, GraphNodeEventHandler,
    EventType, EventSeverity,
};

#[tokio::test]
async fn test_generic_event_handler() {
    let handler = GenericEventHandler::new("test-service".to_string());
    
    // Test error detection
    let event = handler.handle_log_line("ERROR: Something went wrong").await;
    assert!(event.is_some());
    let event = event.unwrap();
    assert_eq!(event.service_name, "test-service");
    assert!(matches!(event.event_type, EventType::Error));
    assert!(matches!(event.severity, EventSeverity::Error));
    
    // Test warning detection
    let event = handler.handle_log_line("WARNING: This is a warning").await;
    assert!(event.is_some());
    let event = event.unwrap();
    assert!(matches!(event.severity, EventSeverity::Warning));
    
    // Test startup detection
    let event = handler.handle_log_line("Service started successfully").await;
    assert!(event.is_some());
    let event = event.unwrap();
    assert!(matches!(event.event_type, EventType::Started));
    
    // Test that debug messages are filtered out
    let event = handler.handle_log_line("Just a normal log line").await;
    assert!(event.is_none());
}

#[tokio::test]
async fn test_postgres_event_handler() {
    let handler = PostgresEventHandler::new();
    
    // Test database ready detection
    let event = handler.handle_log_line("database system is ready to accept connections").await;
    assert!(event.is_some());
    let event = event.unwrap();
    assert_eq!(event.service_name, "postgres");
    assert!(matches!(event.event_type, EventType::Started));
    assert_eq!(event.message, "Database system ready to accept connections");
    
    // Test error detection
    let event = handler.handle_log_line("FATAL: password authentication failed").await;
    assert!(event.is_some());
    let event = event.unwrap();
    assert!(matches!(event.event_type, EventType::Error));
    assert!(matches!(event.severity, EventSeverity::Error));
    
    // Test connection event (should be debug level)
    let event = handler.handle_log_line("connection received from 127.0.0.1").await;
    assert!(event.is_some());
    let event = event.unwrap();
    assert!(matches!(event.event_type, EventType::Network));
    assert!(matches!(event.severity, EventSeverity::Debug));
}

#[tokio::test]
async fn test_graph_node_event_handler() {
    let handler = GraphNodeEventHandler::new();
    
    // Test startup detection
    let event = handler.handle_log_line("Listening on http://0.0.0.0:8000").await;
    assert!(event.is_some());
    let event = event.unwrap();
    assert_eq!(event.service_name, "graph-node");
    assert!(matches!(event.event_type, EventType::Started));
    
    // Test subgraph deployment
    let event = handler.handle_log_line("Subgraph deployed: QmTest123").await;
    assert!(event.is_some());
    let event = event.unwrap();
    assert!(matches!(event.event_type, EventType::StateChange));
    
    // Test sync events
    let event = handler.handle_log_line("Syncing subgraph to block 12345").await;
    assert!(event.is_some());
    let event = event.unwrap();
    assert!(matches!(event.event_type, EventType::StateChange));
    assert!(event.data.is_object());
}

#[tokio::test]
async fn test_event_severity_ordering() {
    // Verify severity ordering
    assert!(EventSeverity::Trace < EventSeverity::Debug);
    assert!(EventSeverity::Debug < EventSeverity::Info);
    assert!(EventSeverity::Info < EventSeverity::Warning);
    assert!(EventSeverity::Warning < EventSeverity::Error);
    assert!(EventSeverity::Error < EventSeverity::Critical);
    
    // Test needs_attention method
    assert!(!EventSeverity::Info.needs_attention());
    assert!(EventSeverity::Warning.needs_attention());
    assert!(EventSeverity::Error.needs_attention());
    assert!(EventSeverity::Critical.needs_attention());
}

#[tokio::test]
async fn test_event_type_helpers() {
    // Test is_problematic method
    assert!(!EventType::Started.is_problematic());
    assert!(!EventType::Stopped.is_problematic());
    assert!(EventType::Error.is_problematic());
    assert!(EventType::Crashed.is_problematic());
}

#[cfg(test)]
mod registry_tests {
    use super::*;
    use local_network_harness::inspection::ServiceEventRegistry;
    
    #[tokio::test]
    async fn test_registry_handler_registration() {
        let mut registry = ServiceEventRegistry::new();
        
        // Register handlers
        registry.register_handler(Box::new(PostgresEventHandler::new()));
        registry.register_handler(Box::new(GraphNodeEventHandler::new()));
        
        // Verify registration
        assert!(registry.has_handler("postgres"));
        assert!(registry.has_handler("graph-node"));
        assert!(!registry.has_handler("unknown-service"));
        
        // Test processing with registered handler
        let event = registry.process_log_line(
            "postgres", 
            "database system is ready"
        ).await;
        assert!(event.is_some());
        
        // Test fallback for unregistered service
        let event = registry.process_log_line(
            "unknown-service",
            "ERROR: Something failed"
        ).await;
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.service_name, "unknown-service");
    }
}