//! Test that YAML configuration files can be parsed correctly

use service_orchestration::StackConfig;
use std::fs;

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
