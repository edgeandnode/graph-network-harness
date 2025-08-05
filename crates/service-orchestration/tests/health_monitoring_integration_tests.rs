//! Integration tests for health monitoring with orchestration

use service_orchestration::{
    DependencyOrchestrator, OrchestrationContext, ServiceConfig, ServiceTarget,
    HealthCheck, HealthMonitoringExt, StackConfig, ServiceInstanceConfig,
    Dependency,
};
use service_registry::Registry;
use std::collections::HashMap;
use std::time::Duration;

#[cfg(feature = "smol")]
#[smol_potat::test]
async fn test_service_with_health_monitoring() {
    // Create a test configuration with health checks
    let mut services = HashMap::new();
    
    // Service that will be healthy
    services.insert(
        "healthy-service".to_string(),
        ServiceInstanceConfig {
            service_type: "process".to_string(),
            orchestration: ServiceConfig {
                name: "healthy-service".to_string(),
                target: ServiceTarget::Process {
                    binary: "true".to_string(), // Always succeeds
                    args: vec![],
                    env: HashMap::new(),
                    working_dir: None,
                },
                dependencies: vec![],
                health_check: Some(HealthCheck {
                    command: "true".to_string(),
                    args: vec![],
                    interval: 1, // Check every second
                    retries: 2,
                    timeout: 5,
                }),
            },
        },
    );
    
    // Service that will become unhealthy
    services.insert(
        "flaky-service".to_string(),
        ServiceInstanceConfig {
            service_type: "process".to_string(),
            orchestration: ServiceConfig {
                name: "flaky-service".to_string(),
                target: ServiceTarget::Process {
                    binary: "echo".to_string(),
                    args: vec!["flaky service".to_string()],
                    env: HashMap::new(),
                    working_dir: None,
                },
                dependencies: vec![Dependency::Service { service: "healthy-service".to_string() }],
                health_check: Some(HealthCheck {
                    command: "false".to_string(), // Always fails
                    args: vec![],
                    interval: 1,
                    retries: 1,
                    timeout: 5,
                }),
            },
        },
    );
    
    let config = StackConfig {
        name: "health-test".to_string(),
        description: Some("Test stack with health monitoring".to_string()),
        services,
        tasks: HashMap::new(),
    };
    
    // Create context and orchestrator
    let registry = Registry::new().await;
    let context = OrchestrationContext::new(config.clone(), registry);
    let context_arc = std::sync::Arc::new(context);
    let orchestrator = DependencyOrchestrator::new((*context_arc).clone(), &config);
    
    // Execute the orchestration
    orchestrator.execute().await.unwrap();
    
    // Wait a bit for health checks to run
    async_runtime_compat::runtime_utils::sleep(Duration::from_secs(5)).await;
    
    // Check health statuses
    let health_manager = context_arc.health_monitoring();
    let health_statuses = health_manager.get_all_health_status();
    
    // Verify healthy service is healthy
    if let Some(status) = health_statuses.get("healthy-service") {
        match status {
            service_orchestration::HealthStatus::Healthy => {
                // Expected
            }
            _ => panic!("Expected healthy-service to be healthy, got: {:?}", status),
        }
    } else {
        panic!("No health status found for healthy-service");
    }
    
    // Verify flaky service is unhealthy
    if let Some(status) = health_statuses.get("flaky-service") {
        match status {
            service_orchestration::HealthStatus::Unhealthy(_) => {
                // Expected
            }
            _ => panic!("Expected flaky-service to be unhealthy, got: {:?}", status),
        }
    } else {
        panic!("No health status found for flaky-service");
    }
    
    // Check registry states reflect health
    let services_list = context_arc.registry().list().await;
    
    let healthy_entry = services_list
        .iter()
        .find(|s| s.name == "healthy-service")
        .expect("healthy-service not found in registry");
    assert_eq!(healthy_entry.state, service_registry::ServiceState::Running);
    
    let flaky_entry = services_list
        .iter()
        .find(|s| s.name == "flaky-service")
        .expect("flaky-service not found in registry");
    assert_eq!(flaky_entry.state, service_registry::ServiceState::Failed);
}

#[cfg(feature = "smol")]
#[smol_potat::test]
async fn test_health_monitoring_stop() {
    let config = StackConfig {
        name: "stop-test".to_string(),
        description: None,
        services: HashMap::new(),
        tasks: HashMap::new(),
    };
    
    let registry = Registry::new().await;
    let context = OrchestrationContext::new(config, registry);
    let health_manager = context.health_monitoring();
    
    // Start monitoring a test service
    let service_config = ServiceConfig {
        name: "test-service".to_string(),
        target: ServiceTarget::Process {
            binary: "echo".to_string(),
            args: vec!["test".to_string()],
            env: HashMap::new(),
            working_dir: None,
        },
        dependencies: vec![],
        health_check: Some(HealthCheck {
            command: "true".to_string(),
            args: vec![],
            interval: 1,
            retries: 1,
            timeout: 5,
        }),
    };
    
    health_manager.start_monitoring("test-service".to_string(), service_config).unwrap();
    
    // Wait for monitoring to start
    async_runtime_compat::runtime_utils::sleep(Duration::from_millis(100)).await;
    
    // Verify monitoring is active
    assert!(health_manager.get_health_status("test-service").is_some());
    
    // Stop monitoring
    health_manager.stop_monitoring("test-service");
    
    // Wait for monitoring to stop
    async_runtime_compat::runtime_utils::sleep(Duration::from_millis(200)).await;
    
    // Verify monitoring has stopped
    assert!(health_manager.get_health_status("test-service").is_none());
}