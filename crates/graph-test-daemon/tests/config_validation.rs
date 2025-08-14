//! Integration tests to validate all configuration files can be parsed

use service_orchestration::StackConfig;
use std::fs;
use std::path::Path;

#[test]
fn test_all_configs_load_successfully() {
    let configs_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("configs");

    assert!(configs_dir.exists(), "configs directory should exist");

    let entries = fs::read_dir(&configs_dir).expect("Failed to read configs directory");

    let mut yaml_files = Vec::new();

    for entry in entries {
        let entry = entry.expect("Failed to read directory entry");
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
            yaml_files.push(path);
        }
    }

    assert!(
        !yaml_files.is_empty(),
        "Should have at least one YAML config file"
    );

    for yaml_path in yaml_files {
        let file_name = yaml_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");

        println!("Testing config file: {}", file_name);

        let content = fs::read_to_string(&yaml_path)
            .unwrap_or_else(|_| panic!("Failed to read config file: {}", file_name));

        // Try to parse as StackConfig
        let result: Result<StackConfig, _> = serde_yaml::from_str(&content);

        match result {
            Ok(config) => {
                println!("  ✓ Successfully parsed: {}", config.name);

                // Additional validation
                assert!(!config.name.is_empty(), "Config name should not be empty");

                // Check that all services have valid targets
                for (service_name, service) in &config.services {
                    println!("    - Service: {}", service_name);

                    // Verify service has required fields
                    assert!(
                        !service.service_type.is_empty(),
                        "Service {} should have a service_type",
                        service_name
                    );

                    // Verify target can build command if it's a process
                    if let service_orchestration::ServiceTarget::Process { command, .. } =
                        &service.orchestration.target
                    {
                        let cmd_parts = command.build_command();
                        assert!(
                            !cmd_parts.is_empty(),
                            "Service {} should have a valid command",
                            service_name
                        );
                    }
                }

                // Check tasks if any
                for (task_name, task) in &config.tasks {
                    println!("    - Task: {}", task_name);
                    assert!(
                        !task.task_type.is_empty(),
                        "Task {} should have a task_type",
                        task_name
                    );
                }
            }
            Err(e) => {
                panic!("Failed to parse config file {}: {}", file_name, e);
            }
        }
    }
}

#[test]
fn test_graph_stack_config_specifics() {
    let config_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("configs")
        .join("graph-stack.yaml");

    let content = fs::read_to_string(&config_path).expect("Failed to read graph-stack.yaml");

    let config: StackConfig =
        serde_yaml::from_str(&content).expect("Failed to parse graph-stack.yaml");

    // Verify expected services exist
    assert!(
        config.services.contains_key("postgres"),
        "Should have postgres service"
    );
    assert!(
        config.services.contains_key("ipfs"),
        "Should have ipfs service"
    );
    assert!(
        config.services.contains_key("anvil"),
        "Should have anvil service"
    );
    assert!(
        config.services.contains_key("graph-node"),
        "Should have graph-node service"
    );

    // Verify anvil uses template command
    let anvil = config.services.get("anvil").unwrap();
    if let service_orchestration::ServiceTarget::Process { command, .. } =
        &anvil.orchestration.target
    {
        match command {
            service_orchestration::ProcessCommand::Template {
                params,
                command_template,
                ..
            } => {
                // Check required params exist
                assert!(
                    params.contains_key("binary"),
                    "Anvil should have binary param"
                );
                assert!(params.contains_key("port"), "Anvil should have port param");
                assert!(
                    params.contains_key("chain_id"),
                    "Anvil should have chain_id param"
                );

                // Check template contains expected placeholders
                assert!(
                    command_template.contains("{binary}"),
                    "Template should reference binary"
                );
                assert!(
                    command_template.contains("{port}"),
                    "Template should reference port"
                );
                assert!(
                    command_template.contains("{chain_id}"),
                    "Template should reference chain_id"
                );
            }
            _ => panic!("Anvil should use Template command"),
        }
    }

    // Verify graph-node dependencies
    let graph_node = config.services.get("graph-node").unwrap();
    let deps = &graph_node.orchestration.depends_on;
    assert_eq!(deps.len(), 3, "Graph node should have 3 dependencies");

    let dep_names: Vec<String> = deps
        .iter()
        .map(|d| match d {
            service_orchestration::Dependency::Service { service } => service.clone(),
            service_orchestration::Dependency::Task { task } => task.clone(),
        })
        .collect();

    assert!(
        dep_names.contains(&"postgres".to_string()),
        "Should depend on postgres"
    );
    assert!(
        dep_names.contains(&"ipfs".to_string()),
        "Should depend on ipfs"
    );
    assert!(
        dep_names.contains(&"anvil".to_string()),
        "Should depend on anvil"
    );
}

#[test]
fn test_config_parameter_substitution() {
    let config_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("configs")
        .join("graph-stack.yaml");

    let content = fs::read_to_string(&config_path).expect("Failed to read graph-stack.yaml");

    let config: StackConfig =
        serde_yaml::from_str(&content).expect("Failed to parse graph-stack.yaml");

    // Test postgres parameter substitution
    let postgres = config.services.get("postgres").unwrap();
    if let service_orchestration::ServiceTarget::Docker { params, env, .. } =
        &postgres.orchestration.target
    {
        // Check params are defined
        assert!(
            params.contains_key("user"),
            "Postgres should have user param"
        );
        assert!(
            params.contains_key("password"),
            "Postgres should have password param"
        );
        assert!(
            params.contains_key("database"),
            "Postgres should have database param"
        );

        // Check env uses param substitution
        assert!(
            env.get("POSTGRES_USER").unwrap().contains("{user}"),
            "POSTGRES_USER should reference user param"
        );
        assert!(
            env.get("POSTGRES_PASSWORD").unwrap().contains("{password}"),
            "POSTGRES_PASSWORD should reference password param"
        );
        assert!(
            env.get("POSTGRES_DB").unwrap().contains("{database}"),
            "POSTGRES_DB should reference database param"
        );
    }
}
