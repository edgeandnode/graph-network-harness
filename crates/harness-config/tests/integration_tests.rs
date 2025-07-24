//! Integration tests for harness-config

use harness_config::{parser, Config, Service, ServiceType, Network, HealthCheck, HealthCheckType};
use std::collections::HashMap;

#[test]
fn test_full_config_parsing() {
    let yaml = r#"
version: "1.0"
name: "test-deployment"
description: "Test configuration"

settings:
  log_level: debug
  health_check_interval: 30
  startup_timeout: 300
  shutdown_timeout: 30

networks:
  local:
    type: local
    subnet: "127.0.0.0/8"
  
  lan:
    type: lan
    subnet: "192.168.1.0/24"
    nodes:
      - host: "192.168.1.100"
        name: "worker-1"
        ssh_user: "ubuntu"
        ssh_key: "~/.ssh/id_rsa"

services:
  postgres:
    type: docker
    network: local
    image: "postgres:15"
    ports:
      - 5432
      - "5433:5432"
    volumes:
      - "/data:/var/lib/postgresql/data"
    env:
      POSTGRES_PASSWORD: "secret"
      POSTGRES_DB: "testdb"
    health_check:
      command: "pg_isready"
      args: ["-U", "postgres"]
      interval: 10
      retries: 5
      timeout: 5
      start_period: 30

  api:
    type: process
    network: lan
    binary: "/usr/bin/api-server"
    args: ["--port", "8080"]
    working_dir: "/opt/api"
    env:
      DATABASE_URL: "postgresql://localhost:5432/testdb"
      LOG_LEVEL: "info"
    dependencies:
      - postgres
    startup_timeout: 120
    health_check:
      http: "http://localhost:8080/health"
      interval: 30
      retries: 3
      timeout: 10

  worker:
    type: remote
    network: lan
    host: "192.168.1.100"
    binary: "/opt/worker/bin/worker"
    args: ["--threads", "4"]
    working_dir: "/opt/worker"
    env:
      API_URL: "http://localhost:8080"
    dependencies:
      - api

  metrics:
    type: package
    network: lan
    host: "192.168.1.100"
    package: "./packages/metrics-v1.0.0.tar.gz"
    version: "1.0.0"
    install_path: "/opt/metrics"
    env:
      RETENTION_DAYS: "30"
"#;

    let config = parser::parse_str(yaml).unwrap();
    
    // Check basic fields
    assert_eq!(config.version, "1.0");
    assert_eq!(config.name.as_deref(), Some("test-deployment"));
    assert_eq!(config.description.as_deref(), Some("Test configuration"));
    
    // Check settings
    assert_eq!(config.settings.log_level.as_deref(), Some("debug"));
    assert_eq!(config.settings.health_check_interval, Some(30));
    assert_eq!(config.settings.startup_timeout, Some(300));
    assert_eq!(config.settings.shutdown_timeout, Some(30));
    
    // Check networks
    assert_eq!(config.networks.len(), 2);
    assert!(matches!(config.networks.get("local"), Some(Network::Local { .. })));
    assert!(matches!(config.networks.get("lan"), Some(Network::Lan { .. })));
    
    // Check services
    assert_eq!(config.services.len(), 4);
    
    // Check Docker service
    let postgres = config.services.get("postgres").unwrap();
    assert_eq!(postgres.network, "local");
    assert!(matches!(&postgres.service_type, ServiceType::Docker { image, .. } if image == "postgres:15"));
    assert_eq!(postgres.env.get("POSTGRES_PASSWORD"), Some(&"secret".to_string()));
    
    // Check health check
    assert!(postgres.health_check.is_some());
    let hc = postgres.health_check.as_ref().unwrap();
    assert!(matches!(&hc.check_type, HealthCheckType::Command { command, .. } if command == "pg_isready"));
    assert_eq!(hc.interval, 10);
    assert_eq!(hc.retries, 5);
    assert_eq!(hc.timeout, 5);
    assert_eq!(hc.start_period, 30);
    
    // Check dependencies
    let api = config.services.get("api").unwrap();
    assert_eq!(api.dependencies, vec!["postgres"]);
    
    let worker = config.services.get("worker").unwrap();
    assert_eq!(worker.dependencies, vec!["api"]);
}

#[test]
fn test_orchestrator_conversion() {
    let yaml = r#"
version: "1.0"
networks:
  local:
    type: local
services:
  test-service:
    type: process
    network: local
    binary: "/usr/bin/test"
    args: ["arg1", "arg2"]
    env:
      KEY1: "value1"
      KEY2: "value2"
    working_dir: "/tmp"
    dependencies: []
    health_check:
      command: "test"
      args: ["-e", "/tmp/healthy"]
      interval: 5
      retries: 3
      timeout: 2
"#;

    let config = parser::parse_str(yaml).unwrap();
    let service_config = parser::convert_to_orchestrator(&config, "test-service").unwrap();
    
    assert_eq!(service_config.name, "test-service");
    assert_eq!(service_config.dependencies.len(), 0);
    assert!(service_config.health_check.is_some());
    
    let hc = service_config.health_check.unwrap();
    assert_eq!(hc.command, "test");
    assert_eq!(hc.args, vec!["-e", "/tmp/healthy"]);
    assert_eq!(hc.interval, 5);
    assert_eq!(hc.retries, 3);
    assert_eq!(hc.timeout, 2);
}

#[test]
fn test_validation_errors() {
    // Test invalid version
    let yaml = r#"
version: "2.0"
networks:
  local:
    type: local
services: {}
"#;
    
    let result = parser::parse_str(yaml);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Unsupported version"));
    
    // Test missing network reference
    let yaml = r#"
version: "1.0"
networks:
  local:
    type: local
services:
  test:
    type: process
    network: nonexistent
    binary: "/usr/bin/test"
"#;
    
    let result = parser::parse_str(yaml);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("unknown network"));
    
    // Test missing dependency
    let yaml = r#"
version: "1.0"
networks:
  local:
    type: local
services:
  test:
    type: process
    network: local
    binary: "/usr/bin/test"
    dependencies:
      - nonexistent
"#;
    
    let result = parser::parse_str(yaml);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("unknown service"));
}

#[test]
fn test_tcp_health_check() {
    let yaml = r#"
version: "1.0"
networks:
  local:
    type: local
services:
  redis:
    type: docker
    network: local
    image: "redis:7"
    health_check:
      tcp:
        port: 6379
        timeout: 5
      interval: 10
      retries: 3
"#;

    let config = parser::parse_str(yaml).unwrap();
    let service = config.services.get("redis").unwrap();
    
    assert!(service.health_check.is_some());
    let hc = service.health_check.as_ref().unwrap();
    
    // Verify TCP check is converted to nc command
    let orchestrator_config = parser::convert_to_orchestrator(&config, "redis").unwrap();
    let orch_hc = orchestrator_config.health_check.unwrap();
    assert_eq!(orch_hc.command, "nc");
    assert_eq!(orch_hc.args, vec!["-z", "localhost", "6379"]);
}

#[test]
fn test_http_health_check() {
    let yaml = r#"
version: "1.0"
networks:
  local:
    type: local
services:
  api:
    type: process
    network: local
    binary: "/usr/bin/api"
    health_check:
      http: "http://localhost:8080/health"
      interval: 30
      retries: 3
"#;

    let config = parser::parse_str(yaml).unwrap();
    
    // Verify HTTP check is converted to curl command
    let orchestrator_config = parser::convert_to_orchestrator(&config, "api").unwrap();
    let hc = orchestrator_config.health_check.unwrap();
    assert_eq!(hc.command, "curl");
    assert_eq!(hc.args, vec!["-f", "http://localhost:8080/health"]);
}