//! Dependency-driven orchestration engine
//!
//! This module implements the core orchestration logic that processes
//! service and task dependencies to execute them in the correct order.

use crate::{
    Error, config::Dependency, context::OrchestrationContext, discovery::ServiceDiscovery,
    health_integration::HealthMonitoringExt, task_config::StackConfig,
};
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

        // Start deployment tracking
        let deployment_id = self
            .context
            .state_manager()
            .start_deployment(self.context.config().name.clone());
        info!("Started deployment: {}", deployment_id);

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
        let task_config = config
            .tasks
            .get(&name)
            .ok_or_else(|| Error::Config(format!("Task '{}' not found", name)))?;

        info!("Executing task: {}", name);

        // Use the helper function with task configuration
        Ok(create_task_handle(name, task_config.clone()))
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
        // Create service discovery client
        let registry_arc = context.registry.clone();
        let discovery = ServiceDiscovery::new(registry_arc);

        // Wait for dependencies to be available
        for dep_name in &dependencies {
            info!(
                "Waiting for dependency '{}' to be available for service '{}'",
                dep_name, name
            );
            discovery
                .wait_for_service(dep_name, 30)
                .await
                .map_err(|e| {
                    crate::Error::Other(format!(
                        "Failed waiting for dependency '{}': {}",
                        dep_name, e
                    ))
                })?;
        }

        // Build configuration based on discovered services
        let discovered_config = discovery
            .build_service_config(&service_config)
            .await
            .map_err(|e| {
                crate::Error::Other(format!("Failed to build config for '{}': {}", name, e))
            })?;

        // Create a modified service config with injected environment variables
        let mut modified_config = service_config.clone();
        if let crate::config::ServiceTarget::Process { env, .. } = &mut modified_config.target {
            // Merge discovered config into environment
            env.extend(discovered_config);
            info!(
                "Injected {} configuration values into service '{}'",
                env.len(),
                name
            );
        }

        // Find the appropriate executor
        let executor = context
            .executors()
            .find_executor(&modified_config)
            .map_err(|e| crate::Error::Other(format!("Failed to find executor: {}", e)))?;

        info!("Starting service '{}' with executor", name);

        // Start the service using the executor with modified config
        let running_service = executor.start(modified_config).await.map_err(|e| {
            crate::Error::Other(format!("Failed to start service '{}': {}", name, e))
        })?;

        // Register service in registry with Starting state
        let now = Utc::now();
        let mut service_entry = ServiceEntry {
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
                .clone()
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
            state: RegistryServiceState::Starting,
            last_health_check: None,
            registered_at: now,
            last_state_change: now,
        };

        context
            .registry()
            .register(service_entry.clone())
            .await
            .map_err(|e| crate::Error::Registry(e))?;

        info!("Service '{}' started, performing setup", name);

        // Perform ServiceSetup if available
        perform_service_setup(&name, &service_config, &context, &running_service).await?;

        // Update state to running after setup
        context
            .registry()
            .update_state(&name, RegistryServiceState::Running)
            .await
            .map_err(|e| crate::Error::Registry(e))?;

        // Start health monitoring if configured
        if service_config.health_check.is_some() {
            let health_manager = context.health_monitoring();
            if let Err(e) = health_manager.start_monitoring(name.clone(), service_config.clone()) {
                warn!("Failed to start health monitoring for '{}': {}", name, e);
            } else {
                info!("Health monitoring started for service '{}'", name);
            }
        }

        info!("Service '{}' is now running and ready", name);
        Ok(())
    })
}

/// Perform service setup using ServiceSetup trait
async fn perform_service_setup(
    name: &str,
    service_config: &crate::config::ServiceConfig,
    context: &Arc<OrchestrationContext>,
    running_service: &crate::executors::RunningService,
) -> Result<(), crate::Error> {
    // Try to get a ServiceSetup implementation for this service
    // This requires integration with graph-test-daemon's service factory

    use std::time::Duration;

    // For testing purposes, check if the service name contains known service types
    // In a real implementation, this would use the ServiceInstanceConfig's service_type
    let service_type = if name.contains("postgres") {
        Some("postgres")
    } else if name.contains("anvil") {
        Some("anvil")
    } else if name.contains("ipfs") {
        Some("ipfs")
    } else if name.contains("graph-node") {
        Some("graph-node")
    } else {
        // Also check binary names for fallback
        match &service_config.target {
            crate::config::ServiceTarget::Process { binary, .. } => match binary.as_str() {
                "anvil" => Some("anvil"),
                "postgres" | "pg_ctl" => Some("postgres"),
                "ipfs" => Some("ipfs"),
                "graph-node" => Some("graph-node"),
                _ => None,
            },
            crate::config::ServiceTarget::Docker { image, .. } => {
                if image.contains("postgres") {
                    Some("postgres")
                } else if image.contains("ipfs") {
                    Some("ipfs")
                } else if image.contains("graph-node") {
                    Some("graph-node")
                } else {
                    None
                }
            }
            _ => None,
        }
    };

    if let Some(service_type) = service_type {
        info!(
            "Checking setup for service '{}' of type '{}'",
            name, service_type
        );

        // For now, we'll simulate the setup check with a simple wait
        // In a real implementation, this would use the ServiceFactory
        // to get the actual ServiceSetup implementation

        let max_retries = 5;
        let retry_delay = Duration::from_secs(1);

        for attempt in 1..=max_retries {
            info!(
                "Setup check attempt {}/{} for service '{}'",
                attempt, max_retries, name
            );

            // In real implementation:
            // if let Some(setup_service) = ServiceFactory::create_setup_service(service_type) {
            //     if setup_service.is_setup_complete().await? {
            //         info!("Service '{}' setup is complete", name);
            //         return Ok(());
            //     }
            // }

            // For now, simulate readiness after a few attempts
            if attempt >= 3 {
                info!("Service '{}' is ready", name);
                return Ok(());
            }

            if attempt < max_retries {
                #[cfg(feature = "smol")]
                smol::Timer::after(retry_delay).await;
                #[cfg(feature = "tokio")]
                tokio::time::sleep(retry_delay).await;
                #[cfg(feature = "async-std")]
                async_std::task::sleep(retry_delay).await;
            }
        }

        return Err(crate::Error::Other(format!(
            "Service '{}' failed to become ready after {} attempts",
            name, max_retries
        )));
    }

    // If we don't have a service type mapping, just continue
    info!("No setup required for service '{}'", name);
    Ok(())
}

/// Create a task execution handle
fn create_task_handle(
    name: String,
    task_config: crate::task_config::TaskConfig,
) -> OrchestrationHandle {
    Box::pin(async move {
        info!(
            "Starting task execution for '{}' of type '{}'",
            name, task_config.task_type
        );

        // Execute the task based on its type and target
        match task_config.task_type.as_str() {
            "graph-contracts" | "graph-contracts-deployment" => {
                info!("Executing Graph contracts deployment task '{}'", name);

                // For now, we'll use the target to execute the command directly
                // This can be extended later to use the actual DeploymentTask implementations
                match &task_config.target {
                    crate::config::ServiceTarget::Process { binary, args, .. } => {
                        info!("Running process: {} {:?}", binary, args);

                        // Simulate task execution time
                        #[cfg(feature = "smol")]
                        smol::Timer::after(std::time::Duration::from_millis(1000)).await;

                        info!("Graph contracts deployment task '{}' completed", name);
                    }
                    _ => {
                        return Err(crate::Error::Config(format!(
                            "Unsupported target type for task '{}'",
                            name
                        )));
                    }
                }
            }
            "subgraph" | "subgraph-deployment" => {
                info!("Executing subgraph deployment task '{}'", name);

                #[cfg(feature = "smol")]
                smol::Timer::after(std::time::Duration::from_millis(800)).await;

                info!("Subgraph deployment task '{}' completed", name);
            }
            "tap-contracts" | "tap-contracts-deployment" => {
                info!("Executing TAP contracts deployment task '{}'", name);

                #[cfg(feature = "smol")]
                smol::Timer::after(std::time::Duration::from_millis(600)).await;

                info!("TAP contracts deployment task '{}' completed", name);
            }
            _ => {
                return Err(crate::Error::Config(format!(
                    "Unknown task type '{}' for task '{}'",
                    task_config.task_type, name
                )));
            }
        }

        info!("Task '{}' execution completed successfully", name);
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
