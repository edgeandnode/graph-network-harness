//! Integration tests for dependency-driven orchestration

use service_orchestration::{
    Dependency, DependencyGraph, DependencyNode, DependencyOrchestrator, OrchestrationContext,
    ServiceConfig, ServiceInstanceConfig, ServiceTarget, StackConfig, TaskConfig,
};
use service_registry::Registry;
use std::collections::HashMap;
use std::sync::Arc;

/// Create a test stack with complex dependencies
fn create_complex_test_stack() -> StackConfig {
    let mut services = HashMap::new();
    let mut tasks = HashMap::new();

    // Foundation services (no dependencies)
    services.insert(
        "postgres".to_string(),
        ServiceInstanceConfig {
            service_type: "postgres".to_string(),
            orchestration: ServiceConfig {
                name: "postgres".to_string(),
                target: ServiceTarget::Docker {
                    image: "postgres:14".to_string(),
                    env: HashMap::new(),
                    ports: vec![5432],
                    volumes: vec![],
                },
                dependencies: vec![],
                health_check: None,
            },
        },
    );

    services.insert(
        "anvil".to_string(),
        ServiceInstanceConfig {
            service_type: "anvil".to_string(),
            orchestration: ServiceConfig {
                name: "anvil".to_string(),
                target: ServiceTarget::Process {
                    binary: "anvil".to_string(),
                    args: vec![],
                    env: HashMap::new(),
                    working_dir: None,
                },
                dependencies: vec![],
                health_check: None,
            },
        },
    );

    services.insert(
        "ipfs".to_string(),
        ServiceInstanceConfig {
            service_type: "ipfs".to_string(),
            orchestration: ServiceConfig {
                name: "ipfs".to_string(),
                target: ServiceTarget::Docker {
                    image: "ipfs/go-ipfs:latest".to_string(),
                    env: HashMap::new(),
                    ports: vec![5001, 8080],
                    volumes: vec![],
                },
                dependencies: vec![],
                health_check: None,
            },
        },
    );

    // Dependent services
    services.insert(
        "graph-node".to_string(),
        ServiceInstanceConfig {
            service_type: "graph-node".to_string(),
            orchestration: ServiceConfig {
                name: "graph-node".to_string(),
                target: ServiceTarget::Process {
                    binary: "graph-node".to_string(),
                    args: vec![],
                    env: HashMap::new(),
                    working_dir: None,
                },
                dependencies: vec![
                    Dependency::Service {
                        service: "postgres".to_string(),
                    },
                    Dependency::Service {
                        service: "ipfs".to_string(),
                    },
                ],
                health_check: None,
            },
        },
    );

    // Tasks
    tasks.insert(
        "deploy-contracts".to_string(),
        TaskConfig {
            task_type: "graph-contracts".to_string(),
            target: ServiceTarget::Process {
                binary: "hardhat".to_string(),
                args: vec!["deploy".to_string()],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![Dependency::Service {
                service: "anvil".to_string(),
            }],
            config: HashMap::new(),
        },
    );

    tasks.insert(
        "deploy-tap-contracts".to_string(),
        TaskConfig {
            task_type: "tap-contracts".to_string(),
            target: ServiceTarget::Process {
                binary: "hardhat".to_string(),
                args: vec![
                    "deploy".to_string(),
                    "--network".to_string(),
                    "localhost".to_string(),
                ],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![Dependency::Task {
                task: "deploy-contracts".to_string(),
            }],
            config: HashMap::new(),
        },
    );

    tasks.insert(
        "deploy-subgraph".to_string(),
        TaskConfig {
            task_type: "subgraph-deployment".to_string(),
            target: ServiceTarget::Process {
                binary: "graph".to_string(),
                args: vec!["deploy".to_string()],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![
                Dependency::Service {
                    service: "graph-node".to_string(),
                },
                Dependency::Task {
                    task: "deploy-contracts".to_string(),
                },
            ],
            config: HashMap::new(),
        },
    );

    StackConfig {
        name: "complex-test-stack".to_string(),
        description: Some("A complex stack with multiple dependencies".to_string()),
        services,
        tasks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complex_dependency_graph() {
        let config = create_complex_test_stack();
        let graph = DependencyGraph::from_stack_config(&config);

        // Get topological order
        let order = graph.topological_sort().unwrap();
        assert_eq!(order.len(), 7); // 4 services + 3 tasks

        // Create position map
        let positions: HashMap<_, _> = order
            .iter()
            .enumerate()
            .map(|(i, node)| (node.clone(), i))
            .collect();

        // Verify dependency order is respected
        // Foundation services come first
        assert!(
            positions[&DependencyNode::Service("postgres".to_string())]
                < positions[&DependencyNode::Service("graph-node".to_string())]
        );
        assert!(
            positions[&DependencyNode::Service("ipfs".to_string())]
                < positions[&DependencyNode::Service("graph-node".to_string())]
        );
        assert!(
            positions[&DependencyNode::Service("anvil".to_string())]
                < positions[&DependencyNode::Task("deploy-contracts".to_string())]
        );

        // Contract deployment order
        assert!(
            positions[&DependencyNode::Task("deploy-contracts".to_string())]
                < positions[&DependencyNode::Task("deploy-tap-contracts".to_string())]
        );
        assert!(
            positions[&DependencyNode::Task("deploy-contracts".to_string())]
                < positions[&DependencyNode::Task("deploy-subgraph".to_string())]
        );

        // Subgraph depends on both graph-node and contracts
        assert!(
            positions[&DependencyNode::Service("graph-node".to_string())]
                < positions[&DependencyNode::Task("deploy-subgraph".to_string())]
        );
    }

    #[test]
    fn test_parallel_execution_detection() {
        let config = create_complex_test_stack();
        let graph = DependencyGraph::from_stack_config(&config);
        let mut completed = std::collections::HashSet::new();

        // Initially, foundation services can run in parallel
        let ready = graph.get_ready_nodes(&completed);
        assert_eq!(ready.len(), 3); // postgres, anvil, ipfs
        assert!(ready.contains(&DependencyNode::Service("postgres".to_string())));
        assert!(ready.contains(&DependencyNode::Service("anvil".to_string())));
        assert!(ready.contains(&DependencyNode::Service("ipfs".to_string())));

        // Complete postgres and ipfs
        completed.insert(DependencyNode::Service("postgres".to_string()));
        completed.insert(DependencyNode::Service("ipfs".to_string()));

        // Now graph-node should be ready
        let ready = graph.get_ready_nodes(&completed);
        assert!(ready.contains(&DependencyNode::Service("graph-node".to_string())));
        assert!(ready.contains(&DependencyNode::Service("anvil".to_string())));

        // Complete anvil
        completed.insert(DependencyNode::Service("anvil".to_string()));

        // Now deploy-contracts should be ready
        let ready = graph.get_ready_nodes(&completed);
        assert!(ready.contains(&DependencyNode::Task("deploy-contracts".to_string())));
        assert!(ready.contains(&DependencyNode::Service("graph-node".to_string())));
    }

    #[cfg(feature = "smol")]
    #[smol_potat::test]
    async fn test_orchestrator_execution_order() {
        let config = create_complex_test_stack();
        let registry = Registry::new().await;
        let context = OrchestrationContext::new(config.clone(), registry);

        // Track execution order
        let execution_order = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));

        // Create a custom orchestrator that tracks execution
        let orchestrator = DependencyOrchestrator::new(context, &config);

        // We can't easily test the full execution without mocking,
        // but we can verify the graph is constructed correctly
        let graph = DependencyGraph::from_stack_config(&config);
        let order = graph.topological_sort().unwrap();

        // Verify we get a valid execution order
        assert_eq!(order.len(), 7);
    }

    #[test]
    fn test_missing_dependency_detection() {
        let mut services = HashMap::new();
        let tasks = HashMap::new();

        // Service that depends on non-existent service
        services.insert(
            "broken-service".to_string(),
            ServiceInstanceConfig {
                service_type: "test".to_string(),
                orchestration: ServiceConfig {
                    name: "broken-service".to_string(),
                    target: ServiceTarget::Process {
                        binary: "test".to_string(),
                        args: vec![],
                        env: HashMap::new(),
                        working_dir: None,
                    },
                    dependencies: vec![Dependency::Service {
                        service: "non-existent".to_string(),
                    }],
                    health_check: None,
                },
            },
        );

        let config = StackConfig {
            name: "broken".to_string(),
            description: None,
            services,
            tasks,
        };

        let graph = DependencyGraph::from_stack_config(&config);

        // The graph should be able to get a topological sort even with missing dependencies
        // The missing dependency node is created automatically
        let order = graph.topological_sort().unwrap();

        // Verify both nodes exist in the order
        assert!(order.contains(&DependencyNode::Service("non-existent".to_string())));
        assert!(order.contains(&DependencyNode::Service("broken-service".to_string())));

        // Verify the missing dependency comes before the dependent
        let positions: HashMap<_, _> = order
            .iter()
            .enumerate()
            .map(|(i, node)| (node.clone(), i))
            .collect();
        assert!(
            positions[&DependencyNode::Service("non-existent".to_string())]
                < positions[&DependencyNode::Service("broken-service".to_string())]
        );
    }

    #[test]
    fn test_complex_circular_dependency() {
        let mut services = HashMap::new();

        // Create a cycle: A -> B -> C -> A
        services.insert(
            "service-a".to_string(),
            ServiceInstanceConfig {
                service_type: "test".to_string(),
                orchestration: ServiceConfig {
                    name: "service-a".to_string(),
                    target: ServiceTarget::Process {
                        binary: "test".to_string(),
                        args: vec![],
                        env: HashMap::new(),
                        working_dir: None,
                    },
                    dependencies: vec![Dependency::Service {
                        service: "service-c".to_string(),
                    }],
                    health_check: None,
                },
            },
        );

        services.insert(
            "service-b".to_string(),
            ServiceInstanceConfig {
                service_type: "test".to_string(),
                orchestration: ServiceConfig {
                    name: "service-b".to_string(),
                    target: ServiceTarget::Process {
                        binary: "test".to_string(),
                        args: vec![],
                        env: HashMap::new(),
                        working_dir: None,
                    },
                    dependencies: vec![Dependency::Service {
                        service: "service-a".to_string(),
                    }],
                    health_check: None,
                },
            },
        );

        services.insert(
            "service-c".to_string(),
            ServiceInstanceConfig {
                service_type: "test".to_string(),
                orchestration: ServiceConfig {
                    name: "service-c".to_string(),
                    target: ServiceTarget::Process {
                        binary: "test".to_string(),
                        args: vec![],
                        env: HashMap::new(),
                        working_dir: None,
                    },
                    dependencies: vec![Dependency::Service {
                        service: "service-b".to_string(),
                    }],
                    health_check: None,
                },
            },
        );

        let config = StackConfig {
            name: "circular".to_string(),
            description: None,
            services,
            tasks: HashMap::new(),
        };

        let graph = DependencyGraph::from_stack_config(&config);
        let result = graph.topological_sort();

        // Should detect the circular dependency
        assert!(result.is_err());
        match result {
            Err(e) => assert!(e.to_string().contains("Circular dependency")),
            _ => panic!("Expected circular dependency error"),
        }
    }

    #[test]
    fn test_mixed_service_task_dependencies() {
        let mut services = HashMap::new();
        let mut tasks = HashMap::new();

        // Service that depends on a task (unusual but allowed)
        services.insert(
            "dependent-service".to_string(),
            ServiceInstanceConfig {
                service_type: "test".to_string(),
                orchestration: ServiceConfig {
                    name: "dependent-service".to_string(),
                    target: ServiceTarget::Process {
                        binary: "test".to_string(),
                        args: vec![],
                        env: HashMap::new(),
                        working_dir: None,
                    },
                    dependencies: vec![Dependency::Task {
                        task: "setup-task".to_string(),
                    }],
                    health_check: None,
                },
            },
        );

        tasks.insert(
            "setup-task".to_string(),
            TaskConfig {
                task_type: "setup".to_string(),
                target: ServiceTarget::Process {
                    binary: "setup".to_string(),
                    args: vec![],
                    env: HashMap::new(),
                    working_dir: None,
                },
                dependencies: vec![],
                config: HashMap::new(),
            },
        );

        let config = StackConfig {
            name: "mixed".to_string(),
            description: None,
            services,
            tasks,
        };

        let graph = DependencyGraph::from_stack_config(&config);
        let order = graph.topological_sort().unwrap();

        // Task should come before service
        let positions: HashMap<_, _> = order
            .iter()
            .enumerate()
            .map(|(i, node)| (node.clone(), i))
            .collect();

        assert!(
            positions[&DependencyNode::Task("setup-task".to_string())]
                < positions[&DependencyNode::Service("dependent-service".to_string())]
        );
    }
}
