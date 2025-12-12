//! Integration tests for the orchestrator crate
//!
//! These tests verify that all components work together correctly.

use service_orchestration::{
    ByteSize, CpuLimit, DockerExecutor, HealthCheck, HealthChecker, HealthStatus,
    PortSpec, ProcessCommand, ProcessExecutor, ResourceLimits, ServiceConfig,
    ServiceExecutor, ServiceManager, ServiceStatus, ServiceTarget,
};
use std::collections::HashMap;

#[test]
fn test_service_config_yaml_roundtrip() {
    let config = ServiceConfig {
        name: "test-service".to_string(),
        target: ServiceTarget::Process {
            command: ProcessCommand::Legacy {
                command: "echo hello world".to_string(),
            },
            env: HashMap::from([
                ("LOG_LEVEL".to_string(), "debug".to_string()),
                ("PORT".to_string(), "8080".to_string()),
            ]),
            ports: HashMap::new(),
            resources: None,
            working_dir: Some("/tmp".to_string()),
            validation: None,
        },
        depends_on: vec![
            service_orchestration::Dependency::Service {
                service: "database".to_string(),
            },
            service_orchestration::Dependency::Service {
                service: "cache".to_string(),
            },
        ],
        health_check: Some(HealthCheck {
            command: "curl".to_string(),
            args: vec!["-f".to_string(), "http://localhost:8080/health".to_string()],
            interval: 30,
            retries: 3,
            timeout: 10,
        }),
    };

    // Test YAML serialization
    let yaml = serde_yaml::to_string(&config).expect("Failed to serialize");
    let deserialized: ServiceConfig = serde_yaml::from_str(&yaml).expect("Failed to deserialize");

    assert_eq!(config.name, deserialized.name);
    assert_eq!(config.depends_on, deserialized.depends_on);
    assert!(matches!(deserialized.target, ServiceTarget::Process { .. }));
}

#[test]
fn test_docker_service_config() {
    let config = ServiceConfig {
        name: "nginx-service".to_string(),
        target: ServiceTarget::Docker {
            params: HashMap::new(),
            image: "nginx:latest".to_string(),
            command_template: None,
            env: HashMap::from([("NGINX_PORT".to_string(), "80".to_string())]),
            ports: vec![80, 443],
            volumes: vec!["/data:/usr/share/nginx/html".to_string()],
        },
        depends_on: vec![],
        health_check: Some(HealthCheck {
            command: "curl".to_string(),
            args: vec!["-f".to_string(), "http://localhost/health".to_string()],
            interval: 15,
            retries: 2,
            timeout: 5,
        }),
    };

    // Test that Docker executor can handle this config
    let executor = DockerExecutor::new();
    assert!(executor.can_handle(&config));

    // Test that other executors cannot handle this config
    let process_executor = ProcessExecutor::new();
    assert!(!process_executor.can_handle(&config));
}

#[test]
fn test_service_target_env_methods() {
    let mut env = HashMap::new();
    env.insert("TEST_VAR".to_string(), "test_value".to_string());

    let target = ServiceTarget::Process {
        command: ProcessCommand::Legacy {
            command: "test".to_string(),
        },
        env: env.clone(),
        ports: HashMap::new(),
        resources: None,
        working_dir: None,
        validation: None,
    };

    // Test env() method
    assert_eq!(target.env(), env);

    // Test with_env() method
    let mut new_env = HashMap::new();
    new_env.insert("NEW_VAR".to_string(), "new_value".to_string());

    let updated_target = target.with_env(new_env.clone());
    assert_eq!(updated_target.env(), new_env);

    // Original target should be unchanged
    assert_eq!(target.env(), env);
}

#[smol_potat::test]
async fn test_health_checker_basic_functionality() {
    let checker = HealthChecker::new();

    // Test successful health check
    let success_config = HealthCheck {
        command: "true".to_string(),
        args: vec![],
        interval: 10,
        retries: 1,
        timeout: 5,
    };

    let result = checker.check_health(&success_config).await.unwrap();
    assert_eq!(result, HealthStatus::Healthy);

    // Test failing health check
    let fail_config = HealthCheck {
        command: "false".to_string(),
        args: vec![],
        interval: 10,
        retries: 1,
        timeout: 5,
    };

    let result = checker.check_health(&fail_config).await.unwrap();
    assert!(matches!(result, HealthStatus::Unhealthy(_)));
}

#[smol_potat::test]
async fn test_service_manager_initialization() {
    let manager = ServiceManager::new().await.unwrap();

    // Test that all executors are registered
    let _process_config = ServiceConfig {
        name: "test-process".to_string(),
        target: ServiceTarget::Process {
            command: ProcessCommand::Legacy {
                command: "echo test".to_string(),
            },
            env: HashMap::new(),
            ports: HashMap::new(),
            resources: None,
            working_dir: None,
            validation: None,
        },
        depends_on: vec![],
        health_check: None,
    };

    let _docker_config = ServiceConfig {
        name: "test-docker".to_string(),
        target: ServiceTarget::Docker {
            params: HashMap::new(),
            image: "hello-world".to_string(),
            command_template: None,
            env: HashMap::new(),
            ports: vec![],
            volumes: vec![],
        },
        depends_on: vec![],
        health_check: None,
    };

    // The manager should be able to find appropriate executors
    // (We can't test the actual service starting without infrastructure)
    assert!(manager.list_services().is_empty());
}

#[test]
fn test_service_status_serialization() {
    let statuses = vec![
        ServiceStatus::Stopped,
        ServiceStatus::Starting,
        ServiceStatus::Running,
        ServiceStatus::Unhealthy,
        ServiceStatus::Failed("Something went wrong".to_string()),
    ];

    for status in statuses {
        let yaml = serde_yaml::to_string(&status).expect("Failed to serialize status");
        let deserialized: ServiceStatus =
            serde_yaml::from_str(&yaml).expect("Failed to deserialize status");

        // Check that serialization/deserialization preserves the status type
        match (&status, &deserialized) {
            (ServiceStatus::Stopped, ServiceStatus::Stopped) => {}
            (ServiceStatus::Starting, ServiceStatus::Starting) => {}
            (ServiceStatus::Running, ServiceStatus::Running) => {}
            (ServiceStatus::Unhealthy, ServiceStatus::Unhealthy) => {}
            (ServiceStatus::Failed(msg1), ServiceStatus::Failed(msg2)) => {
                assert_eq!(msg1, msg2);
            }
            _ => panic!("Status mismatch: {status:?} != {deserialized:?}"),
        }
    }
}

#[test]
fn test_executor_type_detection() {
    let process_executor = ProcessExecutor::new();
    let docker_executor = DockerExecutor::new();

    let process_config = ServiceConfig {
        name: "test".to_string(),
        target: ServiceTarget::Process {
            command: ProcessCommand::Legacy {
                command: "test".to_string(),
            },
            env: HashMap::new(),
            ports: HashMap::new(),
            resources: None,
            working_dir: None,
            validation: None,
        },
        depends_on: vec![],
        health_check: None,
    };

    let docker_config = ServiceConfig {
        name: "test".to_string(),
        target: ServiceTarget::Docker {
            params: HashMap::new(),
            image: "test".to_string(),
            command_template: None,
            env: HashMap::new(),
            ports: vec![],
            volumes: vec![],
        },
        depends_on: vec![],
        health_check: None,
    };

    // Test that each executor only handles its own type
    assert!(process_executor.can_handle(&process_config));
    assert!(!process_executor.can_handle(&docker_config));

    assert!(!docker_executor.can_handle(&process_config));
    assert!(docker_executor.can_handle(&docker_config));
}

#[test]
fn test_service_config_env_injection() {
    let original_env = HashMap::from([("ORIGINAL".to_string(), "value".to_string())]);

    let config = ServiceConfig {
        name: "test-service".to_string(),
        target: ServiceTarget::Process {
            command: ProcessCommand::Legacy {
                command: "test".to_string(),
            },
            env: original_env.clone(),
            ports: HashMap::new(),
            resources: None,
            working_dir: None,
            validation: None,
        },
        depends_on: vec![service_orchestration::Dependency::Service {
            service: "db".to_string(),
        }],
        health_check: None,
    };

    // Test environment injection (simulating network config injection)
    let injected_env = HashMap::from([
        ("DB_ADDR".to_string(), "192.168.1.100".to_string()),
        ("SERVICE_NAME".to_string(), "test-service".to_string()),
    ]);

    let updated_config = config.with_env(injected_env.clone());

    // Original config should be unchanged
    assert_eq!(config.target.env(), original_env);

    // Updated config should have new environment
    assert_eq!(updated_config.target.env(), injected_env);
    assert_eq!(updated_config.name, config.name);
    assert_eq!(updated_config.depends_on, config.depends_on);
}

// ============================================================================
// Port Allocation Integration Tests
// ============================================================================

#[smol_potat::test]
async fn test_service_manager_port_allocation() {
    let manager = ServiceManager::new().await.unwrap();

    // Create services with auto ports
    let postgres_config = ServiceConfig {
        name: "postgres".to_string(),
        target: ServiceTarget::Process {
            command: ProcessCommand::Legacy {
                command: "echo postgres".to_string(),
            },
            env: HashMap::new(),
            ports: HashMap::from([
                ("main".to_string(), PortSpec::Fixed(5432)),
            ]),
            resources: None,
            working_dir: None,
            validation: None,
        },
        depends_on: vec![],
        health_check: None,
    };

    let graph_node_config = ServiceConfig {
        name: "graph-node".to_string(),
        target: ServiceTarget::Process {
            command: ProcessCommand::Legacy {
                command: "echo graph-node".to_string(),
            },
            env: HashMap::from([
                ("DATABASE_URL".to_string(), "postgres://localhost:{postgres.port.main}/graph".to_string()),
            ]),
            ports: HashMap::from([
                ("http".to_string(), PortSpec::Auto),
                ("ws".to_string(), PortSpec::Auto),
            ]),
            resources: None,
            working_dir: None,
            validation: None,
        },
        depends_on: vec![],
        health_check: None,
    };

    // Allocate ports for all services
    let services: Vec<(&str, &ServiceConfig)> = vec![
        ("postgres", &postgres_config),
        ("graph-node", &graph_node_config),
    ];
    manager.allocate_ports_for_services(&services).unwrap();

    // Verify ports were allocated
    let registry = manager.port_registry();

    // Postgres should have fixed port
    assert_eq!(registry.get("postgres").unwrap().get("main"), Some(&5432));

    // Graph-node should have auto-allocated ports in ephemeral range
    let gn_ports = registry.get("graph-node").unwrap();
    let http_port = *gn_ports.get("http").unwrap();
    let ws_port = *gn_ports.get("ws").unwrap();

    assert!(http_port >= 49152, "HTTP port {} not in ephemeral range", http_port);
    assert!(ws_port >= 49152, "WS port {} not in ephemeral range", ws_port);
    assert_ne!(http_port, ws_port, "HTTP and WS ports should be different");
}

#[test]
fn test_port_config_yaml_roundtrip() {
    let config = ServiceConfig {
        name: "test-with-ports".to_string(),
        target: ServiceTarget::Process {
            command: ProcessCommand::Legacy {
                command: "echo test".to_string(),
            },
            env: HashMap::new(),
            ports: HashMap::from([
                ("http".to_string(), PortSpec::Auto),
                ("admin".to_string(), PortSpec::Fixed(8020)),
            ]),
            resources: None,
            working_dir: None,
            validation: None,
        },
        depends_on: vec![],
        health_check: None,
    };

    let yaml = serde_yaml::to_string(&config).expect("Failed to serialize");
    let deserialized: ServiceConfig = serde_yaml::from_str(&yaml).expect("Failed to deserialize");

    if let ServiceTarget::Process { ports, .. } = &deserialized.target {
        assert_eq!(ports.get("http"), Some(&PortSpec::Auto));
        assert_eq!(ports.get("admin"), Some(&PortSpec::Fixed(8020)));
    } else {
        panic!("Expected Process target");
    }
}

// ============================================================================
// Resource Limits Integration Tests
// ============================================================================

#[test]
fn test_resource_limits_yaml_roundtrip() {
    let config = ServiceConfig {
        name: "test-with-resources".to_string(),
        target: ServiceTarget::Process {
            command: ProcessCommand::Legacy {
                command: "echo test".to_string(),
            },
            env: HashMap::new(),
            ports: HashMap::new(),
            resources: Some(ResourceLimits::none()
                .memory(ByteSize::from_gb(2))
                .cpu(CpuLimit::from_percentage(200))),
            working_dir: None,
            validation: None,
        },
        depends_on: vec![],
        health_check: None,
    };

    let yaml = serde_yaml::to_string(&config).expect("Failed to serialize");
    let deserialized: ServiceConfig = serde_yaml::from_str(&yaml).expect("Failed to deserialize");

    if let ServiceTarget::Process { resources, .. } = &deserialized.target {
        let res = resources.as_ref().expect("Resources should be present");
        assert_eq!(res.memory.as_ref().unwrap().to_bytes(), 2 * 1024 * 1024 * 1024);
        assert_eq!(res.cpu.as_ref().unwrap().percentage, 200);
    } else {
        panic!("Expected Process target");
    }
}

#[test]
fn test_resource_limits_systemd_command_generation() {
    let limits = ResourceLimits::none()
        .memory(ByteSize::from_mb(512))
        .cpu(CpuLimit::from_cores(1.5));

    let cmd = limits.to_systemd_command("my-service");

    assert_eq!(cmd[0], "systemd-run");
    assert!(cmd.contains(&"--user".to_string()));
    assert!(cmd.contains(&"--scope".to_string()));
    assert!(cmd.contains(&"--unit=harness-my-service.scope".to_string()));
    assert!(cmd.contains(&"--slice=harness-test.slice".to_string()));
    assert!(cmd.contains(&format!("-p MemoryMax={}", 512 * 1024 * 1024)));
    assert!(cmd.contains(&"-p CPUQuota=150%".to_string()));
    assert_eq!(cmd.last(), Some(&"--".to_string()));
}

#[test]
fn test_full_config_with_ports_and_resources() {
    // Test a realistic config with both ports and resources
    let yaml = r#"
name: graph-node
target:
  type: process
  command: /usr/bin/graph-node
  env:
    RUST_LOG: info
    DATABASE_URL: "postgres://localhost:{postgres.port.main}/graph"
  ports:
    http: auto
    ws: auto
    admin: 8020
  resources:
    memory: 2G
    cpu: 200%
depends_on:
  - service: postgres
  - service: ipfs
health_check:
  command: curl
  args:
    - "-f"
    - "http://localhost:{port.http}/health"
  interval: 30
  retries: 3
  timeout: 10
"#;

    let config: ServiceConfig = serde_yaml::from_str(yaml).expect("Failed to parse YAML");

    assert_eq!(config.name, "graph-node");

    if let ServiceTarget::Process { command, env, ports, resources, .. } = &config.target {
        // Check command
        if let ProcessCommand::Legacy { command: cmd } = command {
            assert_eq!(cmd, "/usr/bin/graph-node");
        } else {
            panic!("Expected Legacy command");
        }

        // Check env has port reference
        assert!(env.get("DATABASE_URL").unwrap().contains("{postgres.port.main}"));

        // Check ports
        assert_eq!(ports.get("http"), Some(&PortSpec::Auto));
        assert_eq!(ports.get("ws"), Some(&PortSpec::Auto));
        assert_eq!(ports.get("admin"), Some(&PortSpec::Fixed(8020)));

        // Check resources
        let res = resources.as_ref().expect("Resources should be present");
        assert_eq!(res.memory.as_ref().unwrap().to_bytes(), 2 * 1024 * 1024 * 1024);
        assert_eq!(res.cpu.as_ref().unwrap().percentage, 200);
    } else {
        panic!("Expected Process target");
    }

    // Check health check has port reference
    let health = config.health_check.as_ref().unwrap();
    assert!(health.args.iter().any(|a| a.contains("{port.http}")));
}
