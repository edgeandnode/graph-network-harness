//! Tests for task execution in orchestration

use service_orchestration::{
    DependencyOrchestrator, OrchestrationContext, ServiceConfig, ServiceInstanceConfig,
    ServiceTarget, StackConfig, TaskConfig,
};
use service_registry::Registry;
use std::collections::HashMap;

/// Create a test stack with tasks
fn create_task_test_stack() -> StackConfig {
    let mut services = HashMap::new();
    let mut tasks = HashMap::new();

    // Add anvil service (no dependencies)
    services.insert(
        "anvil".to_string(),
        ServiceInstanceConfig {
            service_type: "anvil".to_string(),
            orchestration: ServiceConfig {
                name: "anvil".to_string(),
                target: ServiceTarget::Process {
                    binary: "echo".to_string(),
                    args: vec!["Anvil blockchain started".to_string()],
                    env: HashMap::new(),
                    working_dir: None,
                },
                dependencies: vec![],
                health_check: None,
            },
        },
    );

    // Add contract deployment task (depends on anvil)
    tasks.insert(
        "deploy-contracts".to_string(),
        TaskConfig {
            task_type: "graph-contracts-deployment".to_string(),
            target: ServiceTarget::Process {
                binary: "hardhat".to_string(),
                args: vec!["deploy".to_string(), "--network".to_string(), "localhost".to_string()],
                env: HashMap::new(),
                working_dir: Some("./contracts".to_string()),
            },
            dependencies: vec![service_orchestration::Dependency::Service {
                service: "anvil".to_string(),
            }],
            config: {
                let mut config = HashMap::new();
                config.insert("ethereum_url".to_string(), serde_json::Value::String("http://localhost:8545".to_string()));
                config.insert("working_dir".to_string(), serde_json::Value::String("./contracts".to_string()));
                config
            },
        },
    );

    // Add subgraph deployment task (depends on contracts)
    tasks.insert(
        "deploy-subgraph".to_string(),
        TaskConfig {
            task_type: "subgraph-deployment".to_string(),
            target: ServiceTarget::Process {
                binary: "graph".to_string(),
                args: vec!["deploy".to_string()],
                env: HashMap::new(),
                working_dir: Some("./subgraph".to_string()),
            },
            dependencies: vec![service_orchestration::Dependency::Task {
                task: "deploy-contracts".to_string(),
            }],
            config: {
                let mut config = HashMap::new();
                config.insert("graph_node_url".to_string(), serde_json::Value::String("http://localhost:8000".to_string()));
                config.insert("ipfs_url".to_string(), serde_json::Value::String("http://localhost:5001".to_string()));
                config
            },
        },
    );

    StackConfig {
        name: "task-test-stack".to_string(),
        description: Some("Test stack with tasks for dependency execution".to_string()),
        services,
        tasks,
    }
}

#[cfg(feature = "smol")]
#[smol_potat::test]
async fn test_task_execution_with_dependencies() {
    let config = create_task_test_stack();
    let registry = Registry::new().await;

    let context = OrchestrationContext::new(config.clone(), registry);
    let orchestrator = DependencyOrchestrator::new(context, &config);

    // Execute the stack with both services and tasks
    let result = orchestrator.execute().await;
    assert!(result.is_ok(), "Orchestration failed: {:?}", result.err());
}

#[cfg(feature = "smol")]
#[smol_potat::test]
async fn test_task_only_execution() {
    let mut tasks = HashMap::new();

    // Add a standalone task
    tasks.insert(
        "standalone-task".to_string(),
        TaskConfig {
            task_type: "tap-contracts-deployment".to_string(),
            target: ServiceTarget::Process {
                binary: "echo".to_string(),
                args: vec!["TAP contracts deployment".to_string()],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![],
            config: HashMap::new(),
        },
    );

    let config = StackConfig {
        name: "task-only-stack".to_string(),
        description: Some("Stack with only tasks".to_string()),
        services: HashMap::new(),
        tasks,
    };

    let registry = Registry::new().await;
    let context = OrchestrationContext::new(config.clone(), registry);
    let orchestrator = DependencyOrchestrator::new(context, &config);

    let result = orchestrator.execute().await;
    assert!(result.is_ok(), "Task-only orchestration failed: {:?}", result.err());
}

#[cfg(feature = "smol")]
#[smol_potat::test]
async fn test_unknown_task_type_fails() {
    let mut tasks = HashMap::new();

    // Add a task with unknown type
    tasks.insert(
        "unknown-task".to_string(),
        TaskConfig {
            task_type: "unknown-task-type".to_string(),
            target: ServiceTarget::Process {
                binary: "echo".to_string(),
                args: vec!["Unknown task".to_string()],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![],
            config: HashMap::new(),
        },
    );

    let config = StackConfig {
        name: "unknown-task-stack".to_string(),
        description: Some("Stack with unknown task type".to_string()),
        services: HashMap::new(),
        tasks,
    };

    let registry = Registry::new().await;
    let context = OrchestrationContext::new(config.clone(), registry);
    let orchestrator = DependencyOrchestrator::new(context, &config);

    let result = orchestrator.execute().await;
    assert!(result.is_err(), "Expected orchestration to fail with unknown task type");
}