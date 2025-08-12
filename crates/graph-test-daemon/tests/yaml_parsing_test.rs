//! Test that YAML configuration files can be parsed correctly

use service_orchestration::StackConfig;
use std::fs;

#[test]
fn test_parse_graph_stack_with_tasks_yaml() {
    let yaml_content = fs::read_to_string("configs/graph-stack-with-tasks.yaml")
        .expect("Failed to read YAML file");

    let config: StackConfig = serde_yaml::from_str(&yaml_content).expect("Failed to parse YAML");

    // Verify basic structure
    assert_eq!(config.name, "graph-test-stack-with-tasks");
    assert!(config.description.is_some());

    // Verify tasks were parsed
    assert!(!config.tasks.is_empty());
    assert!(config.tasks.contains_key("deploy-graph-contracts"));
    assert!(config.tasks.contains_key("deploy-tap-contracts"));

    // Verify services were parsed
    assert!(!config.services.is_empty());
    assert!(config.services.contains_key("postgres"));
    assert!(config.services.contains_key("ipfs"));
    assert!(config.services.contains_key("anvil"));
    assert!(config.services.contains_key("graph-node"));

    // Verify task dependencies
    let graph_contracts_task = &config.tasks["deploy-graph-contracts"];
    assert_eq!(graph_contracts_task.dependencies.len(), 1);

    let tap_contracts_task = &config.tasks["deploy-tap-contracts"];
    assert_eq!(tap_contracts_task.dependencies.len(), 2);

    // Verify service dependencies
    let graph_node_service = &config.services["graph-node"];
    assert_eq!(graph_node_service.orchestration.dependencies.len(), 4);
}

#[test]
fn test_parse_graph_stack_yaml() {
    let yaml_content =
        fs::read_to_string("configs/graph-stack.yaml").expect("Failed to read YAML file");

    let config: StackConfig = serde_yaml::from_str(&yaml_content).expect("Failed to parse YAML");

    // Verify basic structure
    assert_eq!(config.name, "graph-test-stack");

    // Verify no tasks in this config
    assert!(config.tasks.is_empty());

    // Verify services were parsed
    assert!(!config.services.is_empty());
    assert!(config.services.contains_key("postgres"));
    assert!(config.services.contains_key("ipfs"));
    assert!(config.services.contains_key("anvil"));
    assert!(config.services.contains_key("graph-node"));
}
