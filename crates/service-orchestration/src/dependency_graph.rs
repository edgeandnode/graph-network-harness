//! Dependency graph for service and task orchestration
//!
//! This module provides a clean, generic dependency resolution system
//! for managing service and task dependencies without any domain-specific knowledge.

use crate::config::Dependency;
use crate::task_config::StackConfig;
use std::collections::{HashMap, HashSet};
use tracing::{debug, info};

/// Node in the dependency graph
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DependencyNode {
    /// A service node
    Service(String),
    /// A task node
    Task(String),
}

impl std::fmt::Display for DependencyNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DependencyNode::Service(name) => write!(f, "service:{}", name),
            DependencyNode::Task(name) => write!(f, "task:{}", name),
        }
    }
}

/// Dependency graph for orchestration
#[derive(Debug)]
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

                // Track reverse edges for finding dependents
                reverse_edges
                    .entry(node.clone())
                    .or_insert_with(Vec::new)
                    .push(dep_node.clone());

                // Ensure dependency node is in the graph
                nodes.insert(dep_node);
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

                // Track reverse edges
                reverse_edges
                    .entry(node.clone())
                    .or_insert_with(Vec::new)
                    .push(dep_node.clone());

                // Ensure dependency node is in the graph
                nodes.insert(dep_node);
            }
        }

        Self {
            edges,
            reverse_edges,
            nodes,
        }
    }

    /// Get nodes that are ready to execute (have no pending dependencies)
    pub fn get_ready_nodes(&self, completed: &HashSet<DependencyNode>) -> Vec<DependencyNode> {
        let mut ready = Vec::new();

        for node in &self.nodes {
            if completed.contains(node) {
                continue; // Already completed
            }

            // Check if all dependencies are completed
            let dependencies = self.reverse_edges.get(node).map(|v| v.as_slice()).unwrap_or(&[]);
            
            if dependencies.iter().all(|dep| completed.contains(dep)) {
                ready.push(node.clone());
            }
        }

        ready
    }

    /// Get a topological sort of the dependency graph
    pub fn topological_sort(&self) -> Result<Vec<DependencyNode>, String> {
        let mut visited = HashSet::new();
        let mut stack = Vec::new();
        let mut temp_visited = HashSet::new();

        // Run DFS from each node
        for node in &self.nodes {
            if !visited.contains(node) {
                self.topological_sort_dfs(node, &mut visited, &mut temp_visited, &mut stack)?;
            }
        }

        // Reverse to get correct order
        stack.reverse();
        
        debug!("Topological sort order: {:?}", stack);
        Ok(stack)
    }

    /// DFS helper for topological sort
    fn topological_sort_dfs(
        &self,
        node: &DependencyNode,
        visited: &mut HashSet<DependencyNode>,
        temp_visited: &mut HashSet<DependencyNode>,
        stack: &mut Vec<DependencyNode>,
    ) -> Result<(), String> {
        if temp_visited.contains(node) {
            return Err(format!("Circular dependency detected involving {}", node));
        }

        if visited.contains(node) {
            return Ok(());
        }

        temp_visited.insert(node.clone());

        // Visit all nodes that depend on this one
        if let Some(dependents) = self.edges.get(node) {
            for dependent in dependents {
                self.topological_sort_dfs(dependent, visited, temp_visited, stack)?;
            }
        }

        temp_visited.remove(node);
        visited.insert(node.clone());
        stack.push(node.clone());

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProcessCommand, ServiceConfig, ServiceTarget};
    use crate::task_config::{ServiceInstanceConfig, TaskConfig};
    use std::collections::HashMap;

    fn create_test_config() -> StackConfig {
        let mut services = HashMap::new();
        let mut tasks = HashMap::new();

        // Service A (no dependencies)
        services.insert(
            "service-a".to_string(),
            ServiceInstanceConfig {
                service_type: "type-a".to_string(),
                orchestration: ServiceConfig {
                    name: "service-a".to_string(),
                    target: ServiceTarget::Process {
                        command: ProcessCommand::Legacy {
                            command: "test-service-a".to_string(),
                        },
                        env: HashMap::new(),
                        working_dir: None,
                    },
                    dependencies: vec![],
                    health_check: None,
                },
            },
        );

        // Service B (no dependencies)
        services.insert(
            "service-b".to_string(),
            ServiceInstanceConfig {
                service_type: "type-b".to_string(),
                orchestration: ServiceConfig {
                    name: "service-b".to_string(),
                    target: ServiceTarget::Docker {
                        params: HashMap::new(),
                        image: "test-image:latest".to_string(),
                        command_template: None,
                        env: HashMap::new(),
                        ports: vec![8080],
                        volumes: vec![],
                    },
                    dependencies: vec![],
                    health_check: None,
                },
            },
        );

        // Service C (depends on service-b)
        services.insert(
            "service-c".to_string(),
            ServiceInstanceConfig {
                service_type: "type-c".to_string(),
                orchestration: ServiceConfig {
                    name: "service-c".to_string(),
                    target: ServiceTarget::Process {
                        command: ProcessCommand::Legacy {
                            command: "test-service-c".to_string(),
                        },
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

        // Task 1 (depends on service-a)
        tasks.insert(
            "task-1".to_string(),
            TaskConfig {
                task_type: "task-type-1".to_string(),
                target: ServiceTarget::Process {
                    command: ProcessCommand::Legacy {
                        command: "run-task-1".to_string(),
                    },
                    env: HashMap::new(),
                    working_dir: None,
                },
                dependencies: vec![Dependency::Service {
                    service: "service-a".to_string(),
                }],
                config: HashMap::new(),
            },
        );

        // Task 2 (depends on service-c and task-1)
        tasks.insert(
            "task-2".to_string(),
            TaskConfig {
                task_type: "task-type-2".to_string(),
                target: ServiceTarget::Process {
                    command: ProcessCommand::Legacy {
                        command: "run-task-2".to_string(),
                    },
                    env: HashMap::new(),
                    working_dir: None,
                },
                dependencies: vec![
                    Dependency::Service {
                        service: "service-c".to_string(),
                    },
                    Dependency::Task {
                        task: "task-1".to_string(),
                    },
                ],
                config: HashMap::new(),
            },
        );

        StackConfig {
            name: "test-stack".to_string(),
            description: Some("Test stack configuration".to_string()),
            services,
            tasks,
        }
    }

    #[test]
    fn test_dependency_graph_creation() {
        let config = create_test_config();
        let graph = DependencyGraph::from_stack_config(&config);

        // Check all nodes are present
        assert!(graph.nodes.contains(&DependencyNode::Service("service-a".to_string())));
        assert!(graph.nodes.contains(&DependencyNode::Service("service-b".to_string())));
        assert!(graph.nodes.contains(&DependencyNode::Service("service-c".to_string())));
        assert!(graph.nodes.contains(&DependencyNode::Task("task-1".to_string())));
        assert!(graph.nodes.contains(&DependencyNode::Task("task-2".to_string())));
    }

    #[test]
    fn test_topological_sort() {
        let config = create_test_config();
        let graph = DependencyGraph::from_stack_config(&config);

        let sorted = graph.topological_sort().expect("Should not have cycles");

        // Create a position map for easier checking
        let positions: HashMap<_, _> = sorted
            .iter()
            .enumerate()
            .map(|(i, node)| (node.clone(), i))
            .collect();

        // service-a and service-b should come before their dependents
        assert!(
            positions[&DependencyNode::Service("service-a".to_string())]
                < positions[&DependencyNode::Task("task-1".to_string())]
        );
        assert!(
            positions[&DependencyNode::Service("service-b".to_string())]
                < positions[&DependencyNode::Service("service-c".to_string())]
        );
        assert!(
            positions[&DependencyNode::Service("service-c".to_string())]
                < positions[&DependencyNode::Task("task-2".to_string())]
        );
        assert!(
            positions[&DependencyNode::Task("task-1".to_string())]
                < positions[&DependencyNode::Task("task-2".to_string())]
        );
    }

    #[test]
    fn test_get_ready_nodes() {
        let config = create_test_config();
        let graph = DependencyGraph::from_stack_config(&config);

        let mut completed = HashSet::new();

        // Initially, service-a and service-b should be ready (no dependencies)
        let ready = graph.get_ready_nodes(&completed);
        assert_eq!(ready.len(), 2);
        assert!(ready.contains(&DependencyNode::Service("service-a".to_string())));
        assert!(ready.contains(&DependencyNode::Service("service-b".to_string())));

        // Mark service-a as completed
        completed.insert(DependencyNode::Service("service-a".to_string()));
        let ready = graph.get_ready_nodes(&completed);
        assert_eq!(ready.len(), 2);
        assert!(ready.contains(&DependencyNode::Service("service-b".to_string())));
        assert!(ready.contains(&DependencyNode::Task("task-1".to_string())));

        // Mark service-b as completed
        completed.insert(DependencyNode::Service("service-b".to_string()));
        let ready = graph.get_ready_nodes(&completed);
        assert_eq!(ready.len(), 2);
        assert!(ready.contains(&DependencyNode::Task("task-1".to_string())));
        assert!(ready.contains(&DependencyNode::Service("service-c".to_string())));
    }
}