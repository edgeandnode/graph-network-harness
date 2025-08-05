//! Dependency-driven orchestration engine
//!
//! This module implements the core orchestration logic that processes
//! service and task dependencies to execute them in the correct order.

use crate::{Error, config::Dependency, context::OrchestrationContext, task_config::StackConfig};
use chrono::Utc;
use service_registry::{
    ExecutionInfo, Location, ServiceEntry, ServiceState as RegistryServiceState,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Represents a node in the dependency graph
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DependencyNode {
    /// A service instance
    Service(String),
    /// A task instance
    Task(String),
}

impl DependencyNode {
    /// Get the name of the node
    pub fn name(&self) -> &str {
        match self {
            DependencyNode::Service(name) => name,
            DependencyNode::Task(name) => name,
        }
    }
}

/// Dependency graph for orchestration
pub struct DependencyGraph {
    /// Adjacency list representation
    edges: HashMap<DependencyNode, Vec<DependencyNode>>,
    /// Reverse edges for finding dependents
    reverse_edges: HashMap<DependencyNode, Vec<DependencyNode>>,
    /// All nodes in the graph
    nodes: HashSet<DependencyNode>,
}

impl DependencyGraph {
    /// Create a new dependency graph from stack configuration
    pub fn from_stack_config(config: &StackConfig) -> Self {
        let mut edges: HashMap<DependencyNode, Vec<DependencyNode>> = HashMap::new();
        let mut reverse_edges: HashMap<DependencyNode, Vec<DependencyNode>> = HashMap::new();
        let mut nodes = HashSet::new();

        // Add service nodes and their dependencies
        for (name, service_config) in &config.services {
            let node = DependencyNode::Service(name.clone());
            nodes.insert(node.clone());

            // Process dependencies
            for dep in &service_config.orchestration.dependencies {
                let dep_node = match dep {
                    Dependency::Service { service } => DependencyNode::Service(service.clone()),
                    Dependency::Task { task } => DependencyNode::Task(task.clone()),
                };

                // dep_node -> node (dependency must come before this node)
                edges
                    .entry(dep_node.clone())
                    .or_insert_with(Vec::new)
                    .push(node.clone());
                reverse_edges
                    .entry(node.clone())
                    .or_insert_with(Vec::new)
                    .push(dep_node.clone());
            }
        }

        // Add task nodes and their dependencies
        for (name, task_config) in &config.tasks {
            let node = DependencyNode::Task(name.clone());
            nodes.insert(node.clone());

            // Process dependencies
            for dep in &task_config.dependencies {
                let dep_node = match dep {
                    Dependency::Service { service } => DependencyNode::Service(service.clone()),
                    Dependency::Task { task } => DependencyNode::Task(task.clone()),
                };

                // dep_node -> node (dependency must come before this node)
                edges
                    .entry(dep_node.clone())
                    .or_insert_with(Vec::new)
                    .push(node.clone());
                reverse_edges
                    .entry(node.clone())
                    .or_insert_with(Vec::new)
                    .push(dep_node.clone());
            }
        }

        // Ensure all dependency targets exist as nodes
        for deps in edges.values() {
            for dep in deps {
                nodes.insert(dep.clone());
            }
        }

        // Also ensure dependency sources exist as nodes
        for dep_node in edges.keys() {
            nodes.insert(dep_node.clone());
        }

        Self {
            edges,
            reverse_edges,
            nodes,
        }
    }

    /// Perform topological sort to find execution order
    pub fn topological_sort(&self) -> std::result::Result<Vec<DependencyNode>, Error> {
        let mut in_degree: HashMap<&DependencyNode, usize> = HashMap::new();
        let mut queue = VecDeque::new();
        let mut result = Vec::new();

        // Calculate in-degrees
        for node in &self.nodes {
            in_degree.insert(node, 0);
        }
        for deps in self.edges.values() {
            for dep in deps {
                *in_degree.get_mut(&dep).unwrap() += 1;
            }
        }

        // Find nodes with no dependencies
        for (node, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back((*node).clone());
            }
        }

        // Process nodes
        while let Some(node) = queue.pop_front() {
            result.push(node.clone());

            // Process nodes that depend on this one
            if let Some(dependents) = self.edges.get(&node) {
                for dependent in dependents {
                    let degree = in_degree.get_mut(&dependent).unwrap();
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(dependent.clone());
                    }
                }
            }
        }

        // Check for cycles
        if result.len() != self.nodes.len() {
            return Err(Error::Config("Circular dependency detected".to_string()));
        }

        Ok(result)
    }

    /// Get nodes that can be executed in parallel (have all dependencies satisfied)
    pub fn get_ready_nodes(&self, completed: &HashSet<DependencyNode>) -> Vec<DependencyNode> {
        let mut ready = Vec::new();

        for node in &self.nodes {
            if completed.contains(node) {
                continue;
            }

            // Check if all dependencies of this node are completed
            let dependencies = self
                .reverse_edges
                .get(node)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            if dependencies.iter().all(|dep| completed.contains(dep)) {
                ready.push(node.clone());
            }
        }

        ready
    }
}

/// Orchestrator that executes services and tasks based on dependencies
pub struct DependencyOrchestrator {
    context: Arc<OrchestrationContext>,
    graph: DependencyGraph,
}

impl DependencyOrchestrator {
    /// Create a new orchestrator
    pub fn new(context: OrchestrationContext, config: &StackConfig) -> Self {
        Self {
            context: Arc::new(context),
            graph: DependencyGraph::from_stack_config(config),
        }
    }

    /// Execute the stack with dependency resolution
    pub async fn execute(&self) -> std::result::Result<(), Error> {
        info!("Starting dependency-driven orchestration");

        // Verify we can execute in dependency order
        let execution_order = self.graph.topological_sort()?;
        info!("Execution order: {:?}", execution_order);

        let mut completed = HashSet::new();

        // Execute nodes as their dependencies are satisfied
        while completed.len() < self.graph.nodes.len() {
            let ready_nodes = self.graph.get_ready_nodes(&completed);

            if ready_nodes.is_empty() {
                return Err(Error::Config(
                    "No nodes ready to execute but not all completed - possible circular dependency".to_string()
                ));
            }

            info!(
                "Executing {} nodes in parallel: {:?}",
                ready_nodes.len(),
                ready_nodes
            );

            // Execute ready nodes in parallel
            let mut handles = Vec::new();
            for node in ready_nodes {
                let handle = match &node {
                    DependencyNode::Service(name) => self.start_service(name.clone())?,
                    DependencyNode::Task(name) => self.execute_task(name.clone())?,
                };
                handles.push((node.clone(), handle));
            }

            // Wait for all parallel executions to complete
            for (node, handle) in handles {
                match handle.await {
                    Ok(()) => {
                        info!("Completed: {:?}", node);
                        completed.insert(node);
                    }
                    Err(e) => {
                        warn!("Failed to execute {:?}: {}", node, e);
                        return Err(Error::Other(format!(
                            "Execution failed for {:?}: {}",
                            node, e
                        )));
                    }
                }
            }
        }

        info!("Orchestration completed successfully");
        Ok(())
    }

    /// Start a service
    fn start_service(&self, name: String) -> std::result::Result<OrchestrationHandle, Error> {
        let config = self.context.config();
        let service_config = config
            .services
            .get(&name)
            .ok_or_else(|| Error::Config(format!("Service '{}' not found", name)))?;

        info!("Starting service: {}", name);

        // Extract dependencies
        let dependencies = service_config
            .orchestration
            .dependencies
            .iter()
            .filter_map(|dep| match dep {
                Dependency::Service { service } => Some(service.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();

        // Use the helper function with the full context
        Ok(create_service_handle(
            name,
            service_config.orchestration.clone(),
            self.context.clone(),
            dependencies,
        ))
    }

    /// Execute a task
    fn execute_task(&self, name: String) -> std::result::Result<OrchestrationHandle, Error> {
        let config = self.context.config();
        let _task_config = config
            .tasks
            .get(&name)
            .ok_or_else(|| Error::Config(format!("Task '{}' not found", name)))?;

        info!("Executing task: {}", name);

        // Use the helper function
        Ok(create_task_handle(name))
    }
}

/// Handle for an orchestration operation
type OrchestrationHandle = std::pin::Pin<
    Box<dyn std::future::Future<Output = std::result::Result<(), crate::Error>> + Send + 'static>,
>;

/// Create a service startup handle
fn create_service_handle(
    name: String,
    service_config: crate::config::ServiceConfig,
    context: Arc<OrchestrationContext>,
    dependencies: Vec<String>,
) -> OrchestrationHandle {
    Box::pin(async move {
        // Find the appropriate executor
        let executor = context
            .executors()
            .find_executor(&service_config)
            .map_err(|e| crate::Error::Other(format!("Failed to find executor: {}", e)))?;

        info!("Starting service '{}' with executor", name);

        // Start the service using the executor
        let running_service = executor.start(service_config.clone()).await.map_err(|e| {
            crate::Error::Other(format!("Failed to start service '{}': {}", name, e))
        })?;

        // Register service in registry
        let now = Utc::now();
        let service_entry = ServiceEntry {
            name: name.clone(),
            version: "0.1.0".to_string(),
            execution: match running_service.pid {
                Some(pid) => ExecutionInfo::ManagedProcess {
                    pid: Some(pid),
                    command: format!("{:?}", service_config.target),
                    args: vec![],
                },
                None => ExecutionInfo::ManagedProcess {
                    pid: None,
                    command: format!("{:?}", service_config.target),
                    args: vec![],
                },
            },
            location: Location::Local,
            endpoints: running_service
                .endpoints
                .into_iter()
                .filter_map(|(name, addr_str)| {
                    // Try to parse the address string into a SocketAddr
                    addr_str.parse::<std::net::SocketAddr>().ok().map(|addr| {
                        service_registry::Endpoint {
                            name,
                            address: addr,
                            protocol: service_registry::Protocol::Http, // Default to HTTP for now
                            metadata: HashMap::new(),
                        }
                    })
                })
                .collect(),
            depends_on: dependencies,
            state: RegistryServiceState::Running,
            last_health_check: None,
            registered_at: now,
            last_state_change: now,
        };

        context
            .registry()
            .register(service_entry)
            .await
            .map_err(|e| crate::Error::Registry(e))?;

        info!("Service '{}' is now running", name);
        Ok(())
    })
}

/// Create a task execution handle
fn create_task_handle(name: String) -> OrchestrationHandle {
    Box::pin(async move {
        // In a real implementation, this would:
        // 1. Get the task implementation based on task_type
        // 2. Check if already completed (idempotency)
        // 3. Execute the task

        // For now, simulate task execution
        info!("Task '{}' executing...", name);

        // Simulate some work
        #[cfg(feature = "smol")]
        smol::Timer::after(std::time::Duration::from_millis(100)).await;

        info!("Task '{}' completed", name);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ServiceConfig, ServiceTarget};
    use crate::task_config::{ServiceInstanceConfig, TaskConfig};
    use service_registry::Registry;

    fn create_test_config() -> StackConfig {
        let mut services = HashMap::new();
        let mut tasks = HashMap::new();

        // Anvil service (no dependencies)
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

        // Postgres service (no dependencies)
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

        // Graph node (depends on postgres)
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
                    dependencies: vec![Dependency::Service {
                        service: "postgres".to_string(),
                    }],
                    health_check: None,
                },
            },
        );

        // Deploy contracts task (depends on anvil)
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

        // Deploy subgraph task (depends on graph-node and contracts)
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
            name: "test-stack".to_string(),
            description: Some("Test stack with dependencies".to_string()),
            services,
            tasks,
        }
    }

    #[test]
    fn test_dependency_graph_creation() {
        let config = create_test_config();
        let graph = DependencyGraph::from_stack_config(&config);

        // Check nodes were created
        assert_eq!(graph.nodes.len(), 5);
        assert!(
            graph
                .nodes
                .contains(&DependencyNode::Service("anvil".to_string()))
        );
        assert!(
            graph
                .nodes
                .contains(&DependencyNode::Service("postgres".to_string()))
        );
        assert!(
            graph
                .nodes
                .contains(&DependencyNode::Service("graph-node".to_string()))
        );
        assert!(
            graph
                .nodes
                .contains(&DependencyNode::Task("deploy-contracts".to_string()))
        );
        assert!(
            graph
                .nodes
                .contains(&DependencyNode::Task("deploy-subgraph".to_string()))
        );
    }

    #[test]
    fn test_topological_sort() {
        let config = create_test_config();
        let graph = DependencyGraph::from_stack_config(&config);

        let order = graph.topological_sort().unwrap();

        // Verify order respects dependencies
        let positions: HashMap<_, _> = order
            .iter()
            .enumerate()
            .map(|(i, node)| (node.clone(), i))
            .collect();

        // anvil and postgres should come before their dependents
        assert!(
            positions[&DependencyNode::Service("anvil".to_string())]
                < positions[&DependencyNode::Task("deploy-contracts".to_string())]
        );
        assert!(
            positions[&DependencyNode::Service("postgres".to_string())]
                < positions[&DependencyNode::Service("graph-node".to_string())]
        );
        assert!(
            positions[&DependencyNode::Service("graph-node".to_string())]
                < positions[&DependencyNode::Task("deploy-subgraph".to_string())]
        );
        assert!(
            positions[&DependencyNode::Task("deploy-contracts".to_string())]
                < positions[&DependencyNode::Task("deploy-subgraph".to_string())]
        );
    }

    #[test]
    fn test_get_ready_nodes() {
        let config = create_test_config();
        let graph = DependencyGraph::from_stack_config(&config);
        let mut completed = HashSet::new();

        // Initially, only nodes with no dependencies should be ready
        let ready = graph.get_ready_nodes(&completed);
        assert_eq!(ready.len(), 2);
        assert!(ready.contains(&DependencyNode::Service("anvil".to_string())));
        assert!(ready.contains(&DependencyNode::Service("postgres".to_string())));

        // Mark anvil as completed
        completed.insert(DependencyNode::Service("anvil".to_string()));
        let ready = graph.get_ready_nodes(&completed);
        assert_eq!(ready.len(), 2);
        assert!(ready.contains(&DependencyNode::Service("postgres".to_string())));
        assert!(ready.contains(&DependencyNode::Task("deploy-contracts".to_string())));

        // Mark postgres as completed
        completed.insert(DependencyNode::Service("postgres".to_string()));
        let ready = graph.get_ready_nodes(&completed);
        assert_eq!(ready.len(), 2);
        assert!(ready.contains(&DependencyNode::Task("deploy-contracts".to_string())));
        assert!(ready.contains(&DependencyNode::Service("graph-node".to_string())));
    }

    #[test]
    fn test_circular_dependency_detection() {
        let mut services = HashMap::new();

        // Create circular dependency: A -> B -> A
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
                        service: "service-b".to_string(),
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

        let config = StackConfig {
            name: "circular".to_string(),
            description: None,
            services,
            tasks: HashMap::new(),
        };

        let graph = DependencyGraph::from_stack_config(&config);
        let result = graph.topological_sort();
        assert!(result.is_err());
    }

    #[cfg(feature = "smol")]
    #[smol_potat::test]
    async fn test_orchestrator_execution() {
        let config = create_test_config();
        let registry = Registry::new().await;
        let context = OrchestrationContext::new(config.clone(), registry);

        let orchestrator = DependencyOrchestrator::new(context, &config);

        // This would execute the full stack
        // In a real test, we'd mock the service executors
        // For now, just verify it doesn't panic
        let _ = orchestrator.graph.topological_sort();
    }
}
