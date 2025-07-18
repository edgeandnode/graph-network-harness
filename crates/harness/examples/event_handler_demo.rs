//! Simple demonstration of event handlers without running full network

use local_network_harness::inspection::{
    ServiceEventHandler, GenericEventHandler, PostgresEventHandler, GraphNodeEventHandler,
    ServiceEventRegistry,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Service Event Handler Demonstration");
    println!("==================================\n");

    // Create handlers
    let generic_handler = GenericEventHandler::new("unknown-service".to_string());
    let postgres_handler = PostgresEventHandler::new();
    let graph_handler = GraphNodeEventHandler::new();

    // Test generic handler
    println!("Testing Generic Event Handler:");
    println!("------------------------------");
    
    let test_lines = vec![
        "INFO: Service starting up",
        "ERROR: Connection failed",
        "WARNING: Retry attempt 3",
        "Service started successfully",
        "Normal operation log",
    ];
    
    for line in test_lines {
        if let Some(event) = generic_handler.handle_log_line(line).await {
            println!("  ✓ {} → {:?} ({:?})", line, event.event_type, event.severity);
        } else {
            println!("  - {} → (filtered)", line);
        }
    }

    // Test PostgreSQL handler
    println!("\nTesting PostgreSQL Event Handler:");
    println!("---------------------------------");
    
    let postgres_lines = vec![
        "database system is ready to accept connections",
        "connection received from 192.168.1.1",
        "ERROR: relation \"users\" does not exist",
        "FATAL: password authentication failed",
        "LOG: checkpoint starting",
    ];
    
    for line in postgres_lines {
        if let Some(event) = postgres_handler.handle_log_line(line).await {
            println!("  ✓ {} → {:?} ({:?})", line, event.event_type, event.severity);
        } else {
            println!("  - {} → (no event)", line);
        }
    }

    // Test Graph Node handler
    println!("\nTesting Graph Node Event Handler:");
    println!("---------------------------------");
    
    let graph_lines = vec![
        "Listening on http://0.0.0.0:8000",
        "Subgraph deployed: QmXYZ123",
        "Syncing subgraph to block 15000000",
        "Starting indexing for subgraph",
        "Random debug message",
    ];
    
    for line in graph_lines {
        if let Some(event) = graph_handler.handle_log_line(line).await {
            println!("  ✓ {} → {:?} ({:?})", line, event.event_type, event.severity);
        } else {
            println!("  - {} → (no event)", line);
        }
    }

    // Test registry
    println!("\nTesting Service Event Registry:");
    println!("-------------------------------");
    
    let mut registry = ServiceEventRegistry::new();
    registry.register_handler(Box::new(PostgresEventHandler::new()));
    registry.register_handler(Box::new(GraphNodeEventHandler::new()));
    
    println!("Registered services: {:?}", registry.registered_services());
    
    // Test with registered service
    if let Some(event) = registry.process_log_line("postgres", "database system is ready").await {
        println!("  ✓ postgres: {} → {:?}", event.message, event.event_type);
    }
    
    // Test with unregistered service (fallback)
    if let Some(event) = registry.process_log_line("unknown", "ERROR: Something failed").await {
        println!("  ✓ unknown (fallback): {} → {:?}", event.message, event.event_type);
    }

    println!("\nDemo complete!");
    Ok(())
}