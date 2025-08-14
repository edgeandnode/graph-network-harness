//! Integration tests for the LayeredServiceExecutor

use async_runtime_compat::AsyncSpawner;
use service_orchestration::{
    CommandSpec, LayerConfig, LayeredServiceExecutor, ServiceConfig, ServiceExecutor, ServiceTarget,
};
use std::collections::HashMap;

#[smol_potat::test]
async fn test_layered_executor_local_only() {
    let executor = LayeredServiceExecutor::new();

    let config = ServiceConfig {
        name: "test-local-layered".to_string(),
        target: ServiceTarget::Layered {
            params: HashMap::new(),
            layers: vec![LayerConfig::Local {
                env: HashMap::from([("TEST_VAR".to_string(), "test_value".to_string())]),
                working_dir: Some("/tmp".to_string()),
            }],
            command_template: None,
            command: Some(CommandSpec {
                binary: "echo".to_string(),
                args: vec!["hello layered".to_string()],
            }),
            health_check: None,
        },
        depends_on: vec![],
        health_check: None,
    };

    assert!(executor.can_handle(&config));

    let spawner = AsyncSpawner::new();
    let running = executor.start(config, &spawner).await.unwrap();
    assert_eq!(running.name, "test-local-layered");
    assert_eq!(
        running.metadata.get("executor_type"),
        Some(&"layered".to_string())
    );
    assert_eq!(running.metadata.get("layer_count"), Some(&"1".to_string()));

    // Clean up
    executor.stop(&running, &spawner).await.unwrap();
}

#[cfg(feature = "docker-tests")]
#[smol_potat::test]
async fn test_layered_executor_docker_layer() {
    use testcontainers::{clients::Cli, images::generic::GenericImage};

    let docker = Cli::default();
    let container = docker.run(
        GenericImage::new("alpine", "latest")
            .with_wait_for(testcontainers::core::WaitFor::seconds(30)),
    );

    let container_name = format!("alpine-test-{}", container.id());

    let executor = LayeredServiceExecutor::new();

    let config = ServiceConfig {
        name: "test-docker-layered".to_string(),
        target: ServiceTarget::Layered {
            params: HashMap::new(),
            layers: vec![LayerConfig::Docker {
                container: container_name,
                user: None,
                working_dir: Some("/app".to_string()),
                env: HashMap::new(),
                interactive: false,
                tty: false,
            }],
            command_template: None,
            command: Some(CommandSpec {
                binary: "sh".to_string(),
                args: vec!["-c".to_string(), "echo 'hello from docker'".to_string()],
            }),
            health_check: None,
        },
        depends_on: vec![],
        health_check: None,
    };

    let spawner = AsyncSpawner::new();
    let running = executor.start(config, &spawner).await.unwrap();
    assert_eq!(running.name, "test-docker-layered");

    // Clean up
    executor.stop(&running, &spawner).await.unwrap();
}

#[smol_potat::test]
async fn test_layered_executor_rejects_non_layered() {
    let executor = LayeredServiceExecutor::new();

    let config = ServiceConfig {
        name: "test-process".to_string(),
        target: ServiceTarget::Process {
            command: service_orchestration::ProcessCommand::Legacy {
                command: "echo hello".to_string(),
            },
            env: HashMap::new(),
            working_dir: None,
        },
        depends_on: vec![],
        health_check: None,
    };

    assert!(!executor.can_handle(&config));

    let spawner = AsyncSpawner::new();
    let result = executor.start(config, &spawner).await;
    assert!(result.is_err());

    if let Err(e) = result {
        assert!(
            e.to_string()
                .contains("LayeredServiceExecutor can only handle Layered targets")
        );
    }
}

#[test]
fn test_layer_config_serialization() {
    use serde_yaml;

    // Test SSH layer serialization
    let ssh_layer = LayerConfig::Ssh {
        host: "example.com".to_string(),
        user: "deploy".to_string(),
        env: HashMap::from([("KEY".to_string(), "value".to_string())]),
        port: Some(2222),
        identity_file: Some("/home/user/.ssh/id_ed25519".to_string()),
        options: vec!["-o StrictHostKeyChecking=no".to_string()],
    };

    let yaml = serde_yaml::to_string(&ssh_layer).unwrap();
    assert!(yaml.contains("type: ssh"));
    assert!(yaml.contains("host: example.com"));
    assert!(yaml.contains("user: deploy"));
    assert!(yaml.contains("port: 2222"));

    let deserialized: LayerConfig = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(ssh_layer, deserialized);

    // Test Docker layer serialization
    let docker_layer = LayerConfig::Docker {
        container: "my-app".to_string(),
        user: Some("app".to_string()),
        working_dir: Some("/app".to_string()),
        env: HashMap::new(),
        interactive: true,
        tty: true,
    };

    let yaml = serde_yaml::to_string(&docker_layer).unwrap();
    assert!(yaml.contains("type: docker"));
    assert!(yaml.contains("container: my-app"));
    assert!(yaml.contains("user: app"));
    assert!(yaml.contains("interactive: true"));

    let deserialized: LayerConfig = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(docker_layer, deserialized);

    // Test Local layer serialization
    let local_layer = LayerConfig::Local {
        env: HashMap::from([("PATH".to_string(), "/usr/local/bin".to_string())]),
        working_dir: Some("/workspace".to_string()),
    };

    let yaml = serde_yaml::to_string(&local_layer).unwrap();
    assert!(yaml.contains("type: local"));
    assert!(yaml.contains("working_dir: /workspace"));

    let deserialized: LayerConfig = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(local_layer, deserialized);
}

#[test]
fn test_layered_service_config_yaml() {
    use serde_yaml;

    let yaml = r#"
name: complex-service
target:
  type: layered
  layers:
    - type: ssh
      host: jump.example.com
      user: jump
      port: 2222
    - type: docker
      container: app-container
      working_dir: /app
  command:
    binary: node
    args: ["server.js", "--port", "8080"]
depends_on: []
"#;

    let config: ServiceConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.name, "complex-service");

    if let ServiceTarget::Layered {
        layers, command, ..
    } = &config.target
    {
        assert_eq!(layers.len(), 2);

        // Check first layer (SSH)
        if let LayerConfig::Ssh {
            host, user, port, ..
        } = &layers[0]
        {
            assert_eq!(host, "jump.example.com");
            assert_eq!(user, "jump");
            assert_eq!(*port, Some(2222));
        } else {
            panic!("Expected SSH layer");
        }

        // Check second layer (Docker)
        if let LayerConfig::Docker {
            container,
            working_dir,
            ..
        } = &layers[1]
        {
            assert_eq!(container, "app-container");
            assert_eq!(working_dir, &Some("/app".to_string()));
        } else {
            panic!("Expected Docker layer");
        }

        // Check command
        let cmd = command.as_ref().unwrap();
        assert_eq!(cmd.binary, "node");
        assert_eq!(cmd.args, vec!["server.js", "--port", "8080"]);
    } else {
        panic!("Expected Layered target");
    }
}
