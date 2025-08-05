//! Integration tests for ServiceSetup in orchestration

use service_orchestration::{
    DependencyOrchestrator, OrchestrationContext, ServiceConfig, ServiceInstanceConfig,
    ServiceTarget, StackConfig,
};
use service_registry::Registry;
use std::collections::HashMap;

/// Create a test stack with services that need setup
fn create_setup_test_stack() -> StackConfig {
    let mut services = HashMap::new();

    // Add postgres service (foundation service that needs readiness check)
    services.insert(
        "postgres".to_string(),
        ServiceInstanceConfig {
            service_type: "postgres".to_string(),
            orchestration: ServiceConfig {
                name: "postgres".to_string(),
                target: ServiceTarget::Process {
                    binary: "echo".to_string(), // Use echo to simulate postgres
                    args: vec!["PostgreSQL is starting...".to_string()],
                    env: HashMap::new(),
                    working_dir: None,
                },
                dependencies: vec![],
                health_check: None,
            },
        },
    );

    // Add a service that depends on postgres
    services.insert(
        "app".to_string(),
        ServiceInstanceConfig {
            service_type: "app".to_string(),
            orchestration: ServiceConfig {
                name: "app".to_string(),
                target: ServiceTarget::Process {
                    binary: "echo".to_string(),
                    args: vec!["App started after postgres".to_string()],
                    env: HashMap::new(),
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
        name: "setup-test".to_string(),
        description: Some("Test ServiceSetup integration".to_string()),
        services,
        tasks: HashMap::new(),
    }
}

#[cfg(feature = "smol")]
#[smol_potat::test]
async fn test_service_waits_for_setup() {
    let config = create_setup_test_stack();
    let registry = Registry::new().await;

    let context = OrchestrationContext::new(config.clone(), registry);
    let orchestrator = DependencyOrchestrator::new(context.clone(), &config);

    // Execute the orchestration
    let result = orchestrator.execute().await;
    assert!(result.is_ok(), "Orchestration failed: {:?}", result.err());

    // Check that services are registered in the correct order
    let services = context.registry().list().await;

    // Both services should be registered
    assert!(services.iter().any(|s| s.name == "postgres"));
    assert!(services.iter().any(|s| s.name == "app"));

    // Find the services
    let postgres = services.iter().find(|s| s.name == "postgres").unwrap();
    let app = services.iter().find(|s| s.name == "app").unwrap();

    // Both should be in Running state after orchestration
    assert!(matches!(
        postgres.state,
        service_registry::ServiceState::Running
    ));
    assert!(matches!(app.state, service_registry::ServiceState::Running));

    // The app service should have registered after postgres
    // (registration time indicates when service became ready)
    assert!(
        postgres.registered_at <= app.registered_at,
        "App was registered before postgres was ready"
    );
}

#[cfg(feature = "smol")]
#[smol_potat::test]
async fn test_setup_retry_logic() {
    let mut services = HashMap::new();

    // Use postgres service which has a service type mapping and will trigger retry logic
    services.insert(
        "postgres-slow".to_string(),
        ServiceInstanceConfig {
            service_type: "postgres".to_string(),
            orchestration: ServiceConfig {
                name: "postgres-slow".to_string(),
                target: ServiceTarget::Process {
                    binary: "echo".to_string(), // Use echo to avoid missing binary issues
                    args: vec!["PostgreSQL starting slowly...".to_string()],
                    env: HashMap::new(),
                    working_dir: None,
                },
                dependencies: vec![],
                health_check: None,
            },
        },
    );

    let config = StackConfig {
        name: "retry-test".to_string(),
        description: Some("Test setup retry logic".to_string()),
        services,
        tasks: HashMap::new(),
    };

    let registry = Registry::new().await;
    let context = OrchestrationContext::new(config.clone(), registry);
    let orchestrator = DependencyOrchestrator::new(context, &config);

    let start = std::time::Instant::now();
    let result = orchestrator.execute().await;
    let duration = start.elapsed();

    assert!(result.is_ok());
    // Should have waited for retries (simulated 3 attempts with 1 second delays)
    assert!(
        duration.as_secs() >= 2,
        "Setup completed too quickly: {:?}",
        duration
    );
}
