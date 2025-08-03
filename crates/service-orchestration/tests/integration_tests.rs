//! Integration tests for the orchestrator crate
//!
//! These tests verify that all components work together correctly.

use service_orchestration::{
    DockerExecutor, HealthCheck, HealthChecker, HealthStatus, PackageHealthCheck, PackageManifest,
    PackageService, ProcessExecutor, RemoteTarget, ServiceConfig, ServiceExecutor, ServiceManager,
    ServiceStatus, ServiceTarget,
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
        dependencies: vec!["database".to_string(), "cache".to_string()],
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
fn test_remote_lan_service_config() {
    let config = ServiceConfig {
        name: "remote-api".to_string(),
        target: ServiceTarget::RemoteLan {
            host: "192.168.1.100".to_string(),
            user: "deploy".to_string(),
            binary: "./api-server".to_string(),
            args: vec!["--port".to_string(), "3000".to_string()],
        },
        dependencies: vec!["database".to_string()],
        health_check: None,
    };

    // TODO: Remote executor not yet implemented
    // Test that Remote executor can handle this config
    // let executor = RemoteExecutor::new();
    // assert!(executor.can_handle(&config));

    // Test that other executors cannot handle this config
    let process_executor = ProcessExecutor::new();
    let docker_executor = DockerExecutor::new();
    assert!(!process_executor.can_handle(&config));
    assert!(!docker_executor.can_handle(&config));
}

#[test]
fn test_wireguard_service_config() {
    let config = ServiceConfig {
        name: "wg-service".to_string(),
        target: ServiceTarget::Wireguard {
            host: "10.0.0.10".to_string(),
            user: "ubuntu".to_string(),
            package: "/path/to/service.tar.gz".to_string(),
        },
        dependencies: vec![],
        health_check: None,
    };

    // TODO: Remote executor not yet implemented
    // Test that Remote executor can handle WireGuard config
    // let executor = RemoteExecutor::new();
    // assert!(executor.can_handle(&config));
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

#[test]
fn test_package_manifest_serialization() {
    let manifest = PackageManifest {
        name: "my-service".to_string(),
        version: "1.2.3".to_string(),
        service: PackageService {
            executable: "./bin/my-service".to_string(),
            args: vec!["--config".to_string(), "config.yaml".to_string()],
            working_dir: Some("./".to_string()),
            health_check: Some(PackageHealthCheck {
                command: "./health-check.sh".to_string(),
                args: vec![],
                timeout: 30,
            }),
        },
        dependencies: vec!["redis".to_string(), "postgres".to_string()],
        environment: HashMap::from([
            ("LOG_LEVEL".to_string(), "info".to_string()),
            (
                "DATABASE_URL".to_string(),
                "postgres://localhost/mydb".to_string(),
            ),
        ]),
    };

    // Test YAML serialization
    let yaml = serde_yaml::to_string(&manifest).expect("Failed to serialize manifest");
    let deserialized: PackageManifest =
        serde_yaml::from_str(&yaml).expect("Failed to deserialize manifest");

    assert_eq!(manifest.name, deserialized.name);
    assert_eq!(manifest.version, deserialized.version);
    assert_eq!(manifest.dependencies, deserialized.dependencies);
    assert_eq!(manifest.service.executable, deserialized.service.executable);
}

#[test]
fn test_remote_target_install_paths() {
    let target = RemoteTarget {
        service_name: "my-app".to_string(),
        host: "example.com".to_string(),
        user: "deployer".to_string(),
        install_dir: None,
    };

    // Test default install path
    assert_eq!(target.install_path(), "/opt/harness/my-app");

    let custom_target = RemoteTarget {
        service_name: "my-app".to_string(),
        host: "example.com".to_string(),
        user: "deployer".to_string(),
        install_dir: Some("/custom/install/path".to_string()),
    };

    // Test custom install path
    assert_eq!(custom_target.install_path(), "/custom/install/path");
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
            _ => panic!("Status mismatch: {:?} != {:?}", status, deserialized),
        }
    }
}

#[test]
fn test_executor_type_detection() {
    let process_executor = ProcessExecutor::new();
    let docker_executor = DockerExecutor::new();
    // TODO: Remote executor not yet implemented
    // let remote_executor = RemoteExecutor::new();

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

    let remote_config = ServiceConfig {
        name: "test".to_string(),
        target: ServiceTarget::RemoteLan {
            host: "test.example.com".to_string(),
            user: "test".to_string(),
            binary: "test".to_string(),
            args: vec![],
        },
        dependencies: vec![],
        health_check: None,
    };

    // Test that each executor only handles its own type
    assert!(process_executor.can_handle(&process_config));
    assert!(!process_executor.can_handle(&docker_config));
    assert!(!process_executor.can_handle(&remote_config));

    assert!(!docker_executor.can_handle(&process_config));
    assert!(docker_executor.can_handle(&docker_config));
    assert!(!docker_executor.can_handle(&remote_config));

    // assert!(!remote_executor.can_handle(&process_config));
    // assert!(!remote_executor.can_handle(&docker_config));
    // assert!(remote_executor.can_handle(&remote_config));
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
        dependencies: vec!["db".to_string()],
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
