//! Integration tests for the orchestrator crate
//!
//! These tests verify that all components work together correctly.

use service_orchestration::{
    DockerExecutor, HealthCheck, HealthChecker, HealthStatus, LayerConfig, LayeredServiceExecutor,
    ProcessExecutor, ServiceConfig, ServiceExecutor, ServiceManager, ServiceStatus, ServiceTarget,
};
use std::collections::HashMap;

#[test]
fn test_service_config_yaml_roundtrip() {
    let config = ServiceConfig {
        name: "test-service".to_string(),
        target: ServiceTarget::Process {
            binary: "echo".to_string(),
            args: vec!["hello".to_string(), "world".to_string()],
            env: HashMap::from([
                ("LOG_LEVEL".to_string(), "debug".to_string()),
                ("PORT".to_string(), "8080".to_string()),
            ]),
            working_dir: Some("/tmp".to_string()),
        },
        dependencies: vec![
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
    assert_eq!(config.dependencies, deserialized.dependencies);
    assert!(matches!(deserialized.target, ServiceTarget::Process { .. }));
}

#[test]
fn test_docker_service_config() {
    let config = ServiceConfig {
        name: "nginx-service".to_string(),
        target: ServiceTarget::Docker {
            image: "nginx:latest".to_string(),
            env: HashMap::from([("NGINX_PORT".to_string(), "80".to_string())]),
            ports: vec![80, 443],
            volumes: vec!["/data:/usr/share/nginx/html".to_string()],
        },
        dependencies: vec![],
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
    // TODO: Remote executor not yet implemented
    // let remote_executor = RemoteExecutor::new();
    assert!(!process_executor.can_handle(&config));
    // assert!(!remote_executor.can_handle(&config));
}

#[test]
fn test_layered_ssh_service_config() {
    let config = ServiceConfig {
        name: "remote-api".to_string(),
        target: ServiceTarget::Layered {
            layers: vec![
                LayerConfig::Ssh {
                    host: "192.168.1.100".to_string(),
                    user: "deploy".to_string(),
                    env: HashMap::new(),
                    port: None,
                    identity_file: None,
                    options: vec![],
                },
                LayerConfig::Local {
                    env: HashMap::new(),
                    working_dir: None,
                },
            ],
            command: service_orchestration::CommandSpec {
                binary: "./api-server".to_string(),
                args: vec!["--port".to_string(), "3000".to_string()],
            },
        },
        dependencies: vec![service_orchestration::Dependency::Service {
            service: "database".to_string(),
        }],
        health_check: None,
    };

    // Test that LayeredServiceExecutor can handle this config
    let layered_executor = LayeredServiceExecutor::new();
    assert!(layered_executor.can_handle(&config));

    // Test that other executors cannot handle this config
    let process_executor = ProcessExecutor::new();
    let docker_executor = DockerExecutor::new();
    assert!(!process_executor.can_handle(&config));
    assert!(!docker_executor.can_handle(&config));
}

#[test]
fn test_service_target_env_methods() {
    let mut env = HashMap::new();
    env.insert("TEST_VAR".to_string(), "test_value".to_string());

    let target = ServiceTarget::Process {
        binary: "test".to_string(),
        args: vec![],
        env: env.clone(),
        working_dir: None,
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
    let process_config = ServiceConfig {
        name: "test-process".to_string(),
        target: ServiceTarget::Process {
            binary: "echo".to_string(),
            args: vec!["test".to_string()],
            env: HashMap::new(),
            working_dir: None,
        },
        dependencies: vec![],
        health_check: None,
    };

    let docker_config = ServiceConfig {
        name: "test-docker".to_string(),
        target: ServiceTarget::Docker {
            image: "hello-world".to_string(),
            env: HashMap::new(),
            ports: vec![],
            volumes: vec![],
        },
        dependencies: vec![],
        health_check: None,
    };

    // The manager should be able to find appropriate executors
    // (We can't test the actual service starting without infrastructure)
    assert!(manager.list_services().await.unwrap().is_empty());
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
    let layered_executor = LayeredServiceExecutor::new();

    let process_config = ServiceConfig {
        name: "test".to_string(),
        target: ServiceTarget::Process {
            binary: "test".to_string(),
            args: vec![],
            env: HashMap::new(),
            working_dir: None,
        },
        dependencies: vec![],
        health_check: None,
    };

    let docker_config = ServiceConfig {
        name: "test".to_string(),
        target: ServiceTarget::Docker {
            image: "test".to_string(),
            env: HashMap::new(),
            ports: vec![],
            volumes: vec![],
        },
        dependencies: vec![],
        health_check: None,
    };

    let layered_config = ServiceConfig {
        name: "test".to_string(),
        target: ServiceTarget::Layered {
            layers: vec![
                LayerConfig::Ssh {
                    host: "test.example.com".to_string(),
                    user: "test".to_string(),
                    env: HashMap::new(),
                    port: None,
                    identity_file: None,
                    options: vec![],
                },
                LayerConfig::Local {
                    env: HashMap::new(),
                    working_dir: None,
                },
            ],
            command: service_orchestration::CommandSpec {
                binary: "test".to_string(),
                args: vec![],
            },
        },
        dependencies: vec![],
        health_check: None,
    };

    // Test that each executor only handles its own type
    assert!(process_executor.can_handle(&process_config));
    assert!(!process_executor.can_handle(&docker_config));
    assert!(!process_executor.can_handle(&layered_config));

    assert!(!docker_executor.can_handle(&process_config));
    assert!(docker_executor.can_handle(&docker_config));
    assert!(!docker_executor.can_handle(&layered_config));

    assert!(!layered_executor.can_handle(&process_config));
    assert!(!layered_executor.can_handle(&docker_config));
    assert!(layered_executor.can_handle(&layered_config));
}

#[test]
fn test_service_config_env_injection() {
    let original_env = HashMap::from([("ORIGINAL".to_string(), "value".to_string())]);

    let config = ServiceConfig {
        name: "test-service".to_string(),
        target: ServiceTarget::Process {
            binary: "test".to_string(),
            args: vec![],
            env: original_env.clone(),
            working_dir: None,
        },
        dependencies: vec![service_orchestration::Dependency::Service {
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
    assert_eq!(updated_config.dependencies, config.dependencies);
}
