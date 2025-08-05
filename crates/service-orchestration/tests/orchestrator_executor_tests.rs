//! Tests for orchestrator integration with executors

use service_orchestration::{
    Dependency, DependencyOrchestrator, OrchestrationContext, ServiceConfig, ServiceInstanceConfig,
    ServiceTarget, StackConfig,
};
use service_registry::Registry;
use std::collections::HashMap;

/// Create a simple test stack with one service
fn create_test_stack() -> StackConfig {
    let mut services = HashMap::new();

    // Add a simple echo service that sleeps
    services.insert(
        "echo-service".to_string(),
        ServiceInstanceConfig {
            service_type: "echo".to_string(),
            orchestration: ServiceConfig {
                name: "echo-service".to_string(),
                target: ServiceTarget::Process {
                    binary: "echo".to_string(),
                    args: vec!["Starting echo service".to_string()],
                    env: HashMap::new(),
                    working_dir: None,
                },
                dependencies: vec![],
                health_check: None,
            },
        },
    );

    StackConfig {
        name: "test-stack".to_string(),
        description: Some("Test stack for executor integration".to_string()),
        services,
        tasks: HashMap::new(),
    }
}

#[cfg(feature = "smol")]
#[smol_potat::test]
async fn test_orchestrator_starts_service_with_executor() {
    // Create test stack and dependencies
    let config = create_test_stack();
    let registry = Registry::new().await;
    let context = OrchestrationContext::new(config.clone(), registry);

    // Create orchestrator
    let orchestrator = DependencyOrchestrator::new(context, &config);

    // Execute the stack
    let result = orchestrator.execute().await;

    // The echo command should complete successfully
    assert!(result.is_ok(), "Orchestration failed: {:?}", result.err());
}

#[cfg(feature = "smol")]
#[smol_potat::test]
async fn test_orchestrator_with_dependencies() {
    let mut services = HashMap::new();

    // Base service
    services.insert(
        "base".to_string(),
        ServiceInstanceConfig {
            service_type: "echo".to_string(),
            orchestration: ServiceConfig {
                name: "base".to_string(),
                target: ServiceTarget::Process {
                    binary: "echo".to_string(),
                    args: vec!["Base service started".to_string()],
                    env: HashMap::new(),
                    working_dir: None,
                },
                dependencies: vec![],
                health_check: None,
            },
        },
    );

    // Dependent service
    services.insert(
        "dependent".to_string(),
        ServiceInstanceConfig {
            service_type: "echo".to_string(),
            orchestration: ServiceConfig {
                name: "dependent".to_string(),
                target: ServiceTarget::Process {
                    binary: "echo".to_string(),
                    args: vec!["Dependent service started".to_string()],
                    env: HashMap::new(),
                    working_dir: None,
                },
                dependencies: vec![Dependency::Service {
                    service: "base".to_string(),
                }],
                health_check: None,
            },
        },
    );

    let config = StackConfig {
        name: "dependency-test".to_string(),
        description: Some("Test dependency execution order".to_string()),
        services,
        tasks: HashMap::new(),
    };

    let registry = Registry::new().await;
    let context = OrchestrationContext::new(config.clone(), registry);
    let orchestrator = DependencyOrchestrator::new(context, &config);

    // Execute should handle dependencies correctly
    let result = orchestrator.execute().await;
    assert!(result.is_ok(), "Orchestration failed: {:?}", result.err());
}

#[cfg(feature = "smol")]
#[smol_potat::test]
async fn test_orchestrator_handles_invalid_binary() {
    let mut services = HashMap::new();

    // Service with invalid binary
    services.insert(
        "invalid".to_string(),
        ServiceInstanceConfig {
            service_type: "invalid".to_string(),
            orchestration: ServiceConfig {
                name: "invalid".to_string(),
                target: ServiceTarget::Process {
                    binary: "this-binary-does-not-exist".to_string(),
                    args: vec![],
                    env: HashMap::new(),
                    working_dir: None,
                },
                dependencies: vec![],
                health_check: None,
            },
        },
    );

    let config = StackConfig {
        name: "invalid-test".to_string(),
        description: Some("Test invalid binary handling".to_string()),
        services,
        tasks: HashMap::new(),
    };

    let registry = Registry::new().await;
    let context = OrchestrationContext::new(config.clone(), registry);
    let orchestrator = DependencyOrchestrator::new(context, &config);

    // Should fail gracefully
    let result = orchestrator.execute().await;
    assert!(
        result.is_err(),
        "Expected orchestration to fail with invalid binary"
    );
}
