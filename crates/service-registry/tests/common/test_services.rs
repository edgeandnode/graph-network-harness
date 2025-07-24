//! Test service definitions and utilities

use service_registry::{Endpoint, ExecutionInfo, Location, Protocol, ServiceEntry};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use tempfile::TempDir;

/// Create a simple echo service for testing
pub fn create_echo_service() -> anyhow::Result<ServiceEntry> {
    let service = ServiceEntry::new(
        "echo-service".to_string(),
        "1.0.0".to_string(),
        ExecutionInfo::ManagedProcess {
            pid: None,
            command: "echo".to_string(),
            args: vec!["Hello from echo service".to_string()],
        },
        Location::Local,
    )?;

    Ok(service)
}

/// Create a web service with HTTP endpoint
pub fn create_web_service() -> anyhow::Result<ServiceEntry> {
    let mut service = ServiceEntry::new(
        "web-service".to_string(),
        "1.0.0".to_string(),
        ExecutionInfo::ManagedProcess {
            pid: None,
            command: "python3".to_string(),
            args: vec![
                "-m".to_string(),
                "http.server".to_string(),
                "8080".to_string(),
            ],
        },
        Location::Local,
    )?;

    // Add HTTP endpoint
    let endpoint = Endpoint::new(
        "http".to_string(),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
        Protocol::Http,
    );
    service.add_endpoint(endpoint);

    Ok(service)
}

/// Create a Docker-based service
pub fn create_docker_service() -> anyhow::Result<ServiceEntry> {
    let mut service = ServiceEntry::new(
        "nginx-service".to_string(),
        "1.0.0".to_string(),
        ExecutionInfo::DockerContainer {
            container_id: None,
            image: "nginx:alpine".to_string(),
            name: Some("test-nginx".to_string()),
        },
        Location::Local,
    )?;

    // Add HTTP endpoint
    let endpoint = Endpoint::new(
        "http".to_string(),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 80),
        Protocol::Http,
    );
    service.add_endpoint(endpoint);

    Ok(service)
}

/// Create a systemd service
pub fn create_systemd_service() -> anyhow::Result<ServiceEntry> {
    let service = ServiceEntry::new(
        "test-daemon".to_string(),
        "1.0.0".to_string(),
        ExecutionInfo::SystemdService {
            unit_name: "test-daemon.service".to_string(),
        },
        Location::Local,
    )?;

    Ok(service)
}

/// Create a systemd portable service
pub fn create_systemd_portable_service() -> anyhow::Result<ServiceEntry> {
    let service = ServiceEntry::new(
        "portable-service".to_string(),
        "1.0.0".to_string(),
        ExecutionInfo::SystemdPortable {
            image_name: "portable-service".to_string(),
            unit_name: "portable-service.service".to_string(),
        },
        Location::Local,
    )?;

    Ok(service)
}

/// Create a remote SSH service
pub fn create_remote_service() -> anyhow::Result<ServiceEntry> {
    let service = ServiceEntry::new(
        "remote-worker".to_string(),
        "1.0.0".to_string(),
        ExecutionInfo::ManagedProcess {
            pid: None,
            command: "worker".to_string(),
            args: vec![
                "--config".to_string(),
                "/opt/worker/config.yaml".to_string(),
            ],
        },
        Location::Remote {
            host: "worker-node-1".to_string(),
            ssh_user: "deploy".to_string(),
            ssh_port: Some(22),
        },
    )?;

    Ok(service)
}

/// Create a test package directory structure
pub async fn create_test_package(
    temp_dir: &TempDir,
    name: &str,
    version: &str,
) -> anyhow::Result<PathBuf> {
    let package_dir = temp_dir.path().join(format!("{}-{}", name, version));
    let scripts_dir = package_dir.join("scripts");
    let bin_dir = package_dir.join("bin");
    let config_dir = package_dir.join("config");

    // Create directories
    async_fs::create_dir_all(&scripts_dir).await?;
    async_fs::create_dir_all(&bin_dir).await?;
    async_fs::create_dir_all(&config_dir).await?;

    // Create manifest
    let manifest_content = format!(
        r#"
name: "{}"
version: "{}"
description: "Test service package"
service:
  type: "process"
  ports:
    - name: "http"
      port: 8080
      protocol: "http"
depends_on: []
requires:
  commands: ["echo"]
  libraries: []
health:
  script: "scripts/health.sh"
  interval: "30s"
  timeout: "5s"
"#,
        name, version
    );

    async_fs::write(package_dir.join("manifest.yaml"), manifest_content).await?;

    // Create scripts
    let start_script = r#"#!/bin/bash
echo "Starting service..."
echo $$ > /tmp/service.pid
exec sleep 3600
"#;
    async_fs::write(scripts_dir.join("start.sh"), start_script).await?;

    let stop_script = r#"#!/bin/bash
echo "Stopping service..."
if [ -f /tmp/service.pid ]; then
    kill $(cat /tmp/service.pid) || true
    rm -f /tmp/service.pid
fi
"#;
    async_fs::write(scripts_dir.join("stop.sh"), stop_script).await?;

    let health_script = r#"#!/bin/bash
if [ -f /tmp/service.pid ] && kill -0 $(cat /tmp/service.pid) 2>/dev/null; then
    echo "Service is healthy"
    exit 0
else
    echo "Service is not running"
    exit 1
fi
"#;
    async_fs::write(scripts_dir.join("health.sh"), health_script).await?;

    // Create dummy binary
    let binary_content = r#"#!/bin/bash
echo "Test service binary"
"#;
    async_fs::write(bin_dir.join("test-service"), binary_content).await?;

    // Create config file
    let config_content = r#"
# Test service configuration
port: 8080
host: "0.0.0.0"
"#;
    async_fs::write(config_dir.join("config.yaml"), config_content).await?;

    Ok(package_dir)
}

/// Service dependency graph for testing complex deployments
pub struct ServiceDependencyGraph {
    pub services: Vec<ServiceEntry>,
    pub dependencies: Vec<(String, String)>, // (service, depends_on)
}

impl ServiceDependencyGraph {
    /// Create a simple web application stack
    pub fn web_application_stack() -> anyhow::Result<Self> {
        let mut database = ServiceEntry::new(
            "database".to_string(),
            "1.0.0".to_string(),
            ExecutionInfo::DockerContainer {
                container_id: None,
                image: "postgres:13".to_string(),
                name: Some("test-db".to_string()),
            },
            Location::Local,
        )?;

        let db_endpoint = Endpoint::new(
            "postgres".to_string(),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 5432),
            Protocol::Tcp,
        );
        database.add_endpoint(db_endpoint);

        let mut backend = ServiceEntry::new(
            "backend".to_string(),
            "1.0.0".to_string(),
            ExecutionInfo::ManagedProcess {
                pid: None,
                command: "python3".to_string(),
                args: vec!["-m", "flask", "run", "--port=3000"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
            },
            Location::Local,
        )?;

        backend.add_dependency("database".to_string());
        let api_endpoint = Endpoint::new(
            "api".to_string(),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 3000),
            Protocol::Http,
        );
        backend.add_endpoint(api_endpoint);

        let mut frontend = ServiceEntry::new(
            "frontend".to_string(),
            "1.0.0".to_string(),
            ExecutionInfo::DockerContainer {
                container_id: None,
                image: "nginx:alpine".to_string(),
                name: Some("test-frontend".to_string()),
            },
            Location::Local,
        )?;

        frontend.add_dependency("backend".to_string());
        let web_endpoint = Endpoint::new(
            "http".to_string(),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 80),
            Protocol::Http,
        );
        frontend.add_endpoint(web_endpoint);

        Ok(Self {
            services: vec![database, backend, frontend],
            dependencies: vec![
                ("backend".to_string(), "database".to_string()),
                ("frontend".to_string(), "backend".to_string()),
            ],
        })
    }
}
