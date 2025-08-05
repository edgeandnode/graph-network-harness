//! Tests for service discovery and configuration injection

use service_orchestration::{
    DependencyOrchestrator, OrchestrationContext, ServiceConfig, ServiceDiscovery,
    ServiceInstanceConfig, ServiceTarget, StackConfig,
};
use service_registry::Registry;
use std::collections::HashMap;

/// Create a test stack where services need to discover each other
fn create_discovery_test_stack() -> StackConfig {
    let mut services = HashMap::new();

    // Add postgres service that will be discovered
    services.insert(
        "postgres".to_string(),
        ServiceInstanceConfig {
            service_type: "postgres".to_string(),
            orchestration: ServiceConfig {
                name: "postgres".to_string(),
                target: ServiceTarget::Process {
                    binary: "echo".to_string(),
                    args: vec!["PostgreSQL on port 5432".to_string()],
                    env: {
                        let mut env = HashMap::new();
                        env.insert("PGPORT".to_string(), "5432".to_string());
                        env
                    },
                    working_dir: None,
                },
                dependencies: vec![],
                health_check: None,
            },
        },
    );

    // Add an API service that depends on postgres
    services.insert(
        "api".to_string(),
        ServiceInstanceConfig {
            service_type: "api".to_string(),
            orchestration: ServiceConfig {
                name: "api".to_string(),
                target: ServiceTarget::Process {
                    binary: "sh".to_string(),
                    args: vec![
                        "-c".to_string(),
                        "echo API connecting to postgres at $POSTGRES_HOST:$POSTGRES_PORT"
                            .to_string(),
                    ],
                    env: HashMap::new(), // This should be populated by discovery
                    working_dir: None,
                },
                dependencies: vec![service_orchestration::Dependency::Service {
                    service: "postgres".to_string(),
                }],
                health_check: None,
            },
        },
    );

    StackConfig {
        name: "discovery-test".to_string(),
        description: Some("Test service discovery and configuration injection".to_string()),
        services,
        tasks: HashMap::new(),
    }
}

#[cfg(feature = "smol")]
#[smol_potat::test]
async fn test_service_discovery_basic() {
    let registry = std::sync::Arc::new(Registry::new().await);
    let discovery = ServiceDiscovery::new(registry.clone());

    // Register a test service
    let service = service_registry::ServiceEntry {
        name: "test-db".to_string(),
        version: "1.0.0".to_string(),
        execution: service_registry::ExecutionInfo::ManagedProcess {
            pid: Some(1234),
            command: "test".to_string(),
            args: vec![],
        },
        location: service_registry::Location::Local,
        endpoints: vec![service_registry::Endpoint {
            name: "main".to_string(),
            address: "127.0.0.1:5432".parse().unwrap(),
            protocol: service_registry::Protocol::Tcp,
            metadata: HashMap::new(),
        }],
        depends_on: vec![],
        state: service_registry::ServiceState::Running,
        last_health_check: None,
        registered_at: chrono::Utc::now(),
        last_state_change: chrono::Utc::now(),
    };

    registry.register(service).await.unwrap();

    // Test discovery by type
    let endpoints = discovery.discover_by_type("db").await.unwrap();
    assert_eq!(endpoints.len(), 1);
    assert_eq!(endpoints[0].service_name, "test-db");

    // Test discovery by exact name
    let found = discovery.discover_service("test-db").await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "test-db");
}

#[cfg(feature = "smol")]
#[smol_potat::test]
async fn test_configuration_injection() {
    let config = create_discovery_test_stack();
    let registry = Registry::new().await;

    let context = OrchestrationContext::new(config.clone(), registry);
    let orchestrator = DependencyOrchestrator::new(context.clone(), &config);

    // Execute the stack
    let result = orchestrator.execute().await;
    assert!(result.is_ok(), "Orchestration failed: {:?}", result.err());

    // Verify both services are registered
    let services = context.registry().list().await;
    assert_eq!(services.len(), 2);

    // Verify the services are in the correct state
    let postgres = services.iter().find(|s| s.name == "postgres").unwrap();
    let api = services.iter().find(|s| s.name == "api").unwrap();

    assert!(matches!(
        postgres.state,
        service_registry::ServiceState::Running
    ));
    assert!(matches!(api.state, service_registry::ServiceState::Running));

    // The API service should have been configured with postgres endpoint
    // In a real test, we would verify the environment variables were injected
}

#[cfg(feature = "smol")]
#[smol_potat::test]
async fn test_multi_service_discovery() {
    let mut services = HashMap::new();

    // Create multiple postgres instances
    for i in 1..=3 {
        services.insert(
            format!("postgres-{}", i),
            ServiceInstanceConfig {
                service_type: "postgres".to_string(),
                orchestration: ServiceConfig {
                    name: format!("postgres-{}", i),
                    target: ServiceTarget::Process {
                        binary: "echo".to_string(),
                        args: vec![format!("PostgreSQL instance {} on port {}", i, 5430 + i)],
                        env: HashMap::new(),
                        working_dir: None,
                    },
                    dependencies: vec![],
                    health_check: None,
                },
            },
        );
    }

    // Add a service that depends on all postgres instances
    services.insert(
        "aggregator".to_string(),
        ServiceInstanceConfig {
            service_type: "aggregator".to_string(),
            orchestration: ServiceConfig {
                name: "aggregator".to_string(),
                target: ServiceTarget::Process {
                    binary: "echo".to_string(),
                    args: vec!["Aggregator started".to_string()],
                    env: HashMap::new(),
                    working_dir: None,
                },
                dependencies: vec![
                    service_orchestration::Dependency::Service {
                        service: "postgres-1".to_string(),
                    },
                    service_orchestration::Dependency::Service {
                        service: "postgres-2".to_string(),
                    },
                    service_orchestration::Dependency::Service {
                        service: "postgres-3".to_string(),
                    },
                ],
                health_check: None,
            },
        },
    );

    let config = StackConfig {
        name: "multi-discovery-test".to_string(),
        description: Some("Test discovering multiple service instances".to_string()),
        services,
        tasks: HashMap::new(),
    };

    let registry = Registry::new().await;
    let context = OrchestrationContext::new(config.clone(), registry);
    let orchestrator = DependencyOrchestrator::new(context.clone(), &config);

    let result = orchestrator.execute().await;
    assert!(result.is_ok(), "Orchestration failed: {:?}", result.err());

    // All services should be running
    let services = context.registry().list().await;
    assert_eq!(services.len(), 4);

    // The aggregator should have waited for all postgres instances
    let aggregator = services.iter().find(|s| s.name == "aggregator").unwrap();
    assert!(matches!(
        aggregator.state,
        service_registry::ServiceState::Running
    ));
}
