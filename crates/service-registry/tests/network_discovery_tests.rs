//! Network discovery integration tests using Docker Compose

use command_executor::{backends::local::LocalLauncher, Command, Executor, Target};
use service_registry::network::{NetworkConfig, NetworkLocation, NetworkManager, ServiceNetwork};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;
use uuid;

/// Helper to get the path to a docker-compose file
fn compose_file(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/network_tests/docker-compose")
        .join(name)
        .to_string_lossy()
        .to_string()
}

/// Shared Docker-in-Docker container for network discovery tests
/// This container is started once and shared across all tests for efficiency

static DIND_CONTAINER_NAME: &str = "network-discovery-dind-test";
static CONTAINER_GUARD: OnceLock<DindContainerGuard> = OnceLock::new();

struct DindContainerGuard {
    container_name: String,
}

impl Drop for DindContainerGuard {
    fn drop(&mut self) {
        std::process::Command::new("docker")
            .args(&["rm", "-f", &self.container_name])
            .output()
            .ok();
    }
}

/// Ensure the shared Docker-in-Docker container is running
/// This can be called by multiple tests safely - it will only start the container once
async fn ensure_dind_container_running() -> anyhow::Result<()> {
    // Check if container is already running
    let check_cmd = Command::builder("docker")
        .arg("ps")
        .arg("-q")
        .arg("-f")
        .arg(format!("name={}", DIND_CONTAINER_NAME))
        .build();

    let launcher = LocalLauncher;
    let executor = Executor::new("dind-check".to_string(), launcher);
    let result = executor.execute(&Target::Command, check_cmd).await?;

    if !result.output.trim().is_empty() {
        // Container is already running
        return Ok(());
    }

    // Remove any existing container with same name
    let cleanup_cmd = Command::builder("docker")
        .arg("rm")
        .arg("-f")
        .arg(DIND_CONTAINER_NAME)
        .build();
    let _ = executor.execute(&Target::Command, cleanup_cmd).await;

    // Start Docker-in-Docker container with privileged mode
    let docker_cmd = Command::builder("docker")
        .arg("run")
        .arg("-d")
        .arg("--name")
        .arg(DIND_CONTAINER_NAME)
        .arg("--privileged")
        .arg("-e")
        .arg("DOCKER_TLS_CERTDIR=")
        .arg("docker:dind")
        .build();

    let result = executor.execute(&Target::Command, docker_cmd).await?;
    if !result.success() {
        anyhow::bail!(
            "Failed to start Docker-in-Docker container: {:?}",
            result.output
        );
    }

    // Wait for Docker daemon to be ready with retries
    wait_for_docker_daemon_ready(&executor).await?;

    // Copy compose files into container
    copy_compose_files_to_container(&executor).await?;

    // Register cleanup guard
    CONTAINER_GUARD.get_or_init(|| DindContainerGuard {
        container_name: DIND_CONTAINER_NAME.to_string(),
    });

    Ok(())
}

async fn wait_for_docker_daemon_ready(executor: &Executor<LocalLauncher>) -> anyhow::Result<()> {
    let max_attempts = 60; // Increased from 15 to 60

    for i in 1..=max_attempts {
        let health_check = Command::builder("docker")
            .arg("exec")
            .arg(DIND_CONTAINER_NAME)
            .arg("docker")
            .arg("version")
            .build();

        if let Ok(result) = executor.execute(&Target::Command, health_check).await {
            if result.success() {
                return Ok(());
            }
        }

        if i == max_attempts {
            anyhow::bail!(
                "Docker daemon failed to start in container after {} seconds",
                max_attempts
            );
        }

        smol::Timer::after(Duration::from_secs(1)).await;
    }

    Ok(())
}

async fn copy_compose_files_to_container(executor: &Executor<LocalLauncher>) -> anyhow::Result<()> {
    let compose_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/network_tests/docker-compose");

    let cmd = Command::builder("docker")
        .arg("cp")
        .arg(compose_dir.to_str().unwrap())
        .arg(format!("{}:/compose", DIND_CONTAINER_NAME))
        .build();

    let result = executor.execute(&Target::Command, cmd).await?;

    if !result.success() {
        anyhow::bail!(
            "Failed to copy compose files to container: {}",
            result.output
        );
    }

    Ok(())
}

/// Execute a docker-compose command inside the shared DinD container
async fn dind_compose_command(
    compose_file: &str,
    project_name: &str,
    args: Vec<&str>,
) -> anyhow::Result<()> {
    let launcher = LocalLauncher;
    let executor = Executor::new("dind-compose".to_string(), launcher);

    let mut cmd = Command::builder("docker")
        .arg("exec")
        .arg(DIND_CONTAINER_NAME)
        .arg("docker")
        .arg("compose")
        .arg("-f")
        .arg(format!("/compose/{}", compose_file))
        .arg("-p")
        .arg(project_name);

    for arg in args {
        cmd = cmd.arg(arg);
    }

    let result = executor.execute(&Target::Command, cmd.build()).await?;

    if !result.success() {
        anyhow::bail!("Docker compose command failed: {}", result.output);
    }

    Ok(())
}

/// Helper to check if Docker is available using command-executor
async fn check_docker() -> bool {
    let launcher = LocalLauncher;
    let executor = Executor::new("docker-check".to_string(), launcher);
    let cmd = Command::builder("docker").arg("version").build();

    executor
        .execute(&Target::Command, cmd)
        .await
        .map(|r| r.success())
        .unwrap_or(false)
}

#[test]
#[cfg(feature = "docker-tests")]
fn test_local_network_topology() {
    if !smol::block_on(check_docker()) {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    smol::block_on(async {
        // Ensure shared DinD container is running
        ensure_dind_container_running()
            .await
            .expect("Failed to ensure DinD container is running");

        // Use unique project name for isolation within the shared container
        let project_name = format!("test-local-{}", uuid::Uuid::new_v4().simple());

        // Start compose stack in DinD container
        dind_compose_command("local-only.yml", &project_name, vec!["up", "-d"])
            .await
            .expect("Failed to start docker-compose in DinD");

        // Give containers time to start
        smol::Timer::after(Duration::from_secs(2)).await;

        // Create network manager
        let config = NetworkConfig {
            wireguard_subnet: "10.42.0.0/16".parse().unwrap(),
            lan_interface: "eth0".to_string(),
            dns_port: 5353,
            enable_wireguard: true,
        };

        let mut network_manager =
            NetworkManager::new(config).expect("Failed to create network manager");

        // Register local services
        network_manager
            .register_service(ServiceNetwork {
                service_name: "graph-node".to_string(),
                location: NetworkLocation::Local,
                host_ip: Some("172.100.0.20".parse().unwrap()),
                lan_ip: None,
                wireguard_ip: None,
                wireguard_public_key: None,
                interfaces: vec!["docker0".to_string()],
            })
            .await
            .expect("Failed to register graph-node");

        network_manager
            .register_service(ServiceNetwork {
                service_name: "indexer".to_string(),
                location: NetworkLocation::Local,
                host_ip: Some("172.100.0.30".parse().unwrap()),
                lan_ip: None,
                wireguard_ip: None,
                wireguard_public_key: None,
                interfaces: vec!["docker0".to_string()],
            })
            .await
            .expect("Failed to register indexer");

        // Verify no WireGuard needed
        assert!(!network_manager.requires_wireguard());

        // Test IP resolution
        let ip = network_manager
            .resolve_service_ip("graph-node", "indexer")
            .expect("Failed to resolve IP");
        assert_eq!(ip.to_string(), "172.100.0.30");

        // Test environment generation
        let env = network_manager
            .generate_environment("graph-node")
            .expect("Failed to generate environment");
        assert_eq!(env.get("INDEXER_ADDR"), Some(&"172.100.0.30".to_string()));

        // Cleanup - stop compose stack in DinD container
        dind_compose_command("local-only.yml", &project_name, vec!["down", "-v"])
            .await
            .expect("Failed to stop docker-compose in DinD");
    });
}

#[test]
#[cfg(feature = "docker-tests")]
fn test_lan_network_topology() {
    if !smol::block_on(check_docker()) {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    smol::block_on(async {
        // Ensure shared DinD container is running
        ensure_dind_container_running()
            .await
            .expect("Failed to ensure DinD container is running");

        // Use unique project name for isolation within the shared container
        let project_name = format!("test-lan-{}", uuid::Uuid::new_v4().simple());

        // Start compose stack in DinD container
        dind_compose_command("lan-simple.yml", &project_name, vec!["up", "-d"])
            .await
            .expect("Failed to start docker-compose in DinD");

        // Give containers time to start
        smol::Timer::after(Duration::from_secs(2)).await;

        // Use default config from YAML
        let config_yaml = r#"
wireguard_subnet: "10.42.0.0/16"
lan_interface: "eth0"
dns_port: 5353
enable_wireguard: true
"#;

        let config: NetworkConfig =
            serde_yaml::from_str(config_yaml).expect("Failed to parse network config YAML");

        let mut network_manager =
            NetworkManager::new(config).expect("Failed to create network manager");

        // Register LAN services from YAML
        let services_yaml = r#"
- service_name: harness
  location: Local
  host_ip: "127.0.0.1"
  lan_ip: "192.168.100.10"
  interfaces:
    - eth0
- service_name: lan-node-1
  location: !RemoteLAN
    ip: "192.168.100.20"
  lan_ip: "192.168.100.20"
  interfaces:
    - eth0
- service_name: lan-node-2
  location: !RemoteLAN
    ip: "192.168.100.30"
  lan_ip: "192.168.100.30"
  interfaces:
    - eth0
"#;

        let services: Vec<ServiceNetwork> =
            serde_yaml::from_str(services_yaml).expect("Failed to parse services YAML");

        for service in services {
            network_manager
                .register_service(service)
                .await
                .expect("Failed to register service");
        }

        // Verify no WireGuard needed
        assert!(!network_manager.requires_wireguard());

        // Test LAN to LAN resolution
        let ip = network_manager
            .resolve_service_ip("lan-node-1", "lan-node-2")
            .expect("Failed to resolve IP");
        assert_eq!(ip.to_string(), "192.168.100.30");

        // Test harness to LAN resolution
        let ip = network_manager
            .resolve_service_ip("harness", "lan-node-1")
            .expect("Failed to resolve IP");
        assert_eq!(ip.to_string(), "192.168.100.20");

        // Cleanup - stop compose stack in DinD container
        dind_compose_command("lan-simple.yml", &project_name, vec!["down", "-v"])
            .await
            .expect("Failed to stop docker-compose in DinD");
    });
}

#[test]
#[cfg(feature = "docker-tests")]
fn test_mixed_network_topology() {
    if !smol::block_on(check_docker()) {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    smol::block_on(async {
        // Ensure shared DinD container is running
        ensure_dind_container_running()
            .await
            .expect("Failed to ensure DinD container is running");

        // Use unique project name for isolation within the shared container
        let project_name = format!("test-mixed-{}", uuid::Uuid::new_v4().simple());

        // Start compose stack in DinD container
        dind_compose_command("mixed-topology.yml", &project_name, vec!["up", "-d"])
            .await
            .expect("Failed to start docker-compose in DinD");

        // Give containers time to start
        smol::Timer::after(Duration::from_secs(2)).await;

        // Mixed topology configuration from YAML
        let config_yaml = r#"
wireguard_subnet: "10.42.0.0/16"
lan_interface: "eth0"
dns_port: 5353
enable_wireguard: true
"#;

        let config: NetworkConfig =
            serde_yaml::from_str(config_yaml).expect("Failed to parse network config YAML");

        let mut network_manager =
            NetworkManager::new(config).expect("Failed to create network manager");

        // Register services with different network types from YAML
        let services_yaml = r#"
- service_name: local-service
  location: Local
  host_ip: "172.110.0.20"
  interfaces:
    - docker0
- service_name: lan-service
  location: !RemoteLAN
    ip: "192.168.100.20"
  lan_ip: "192.168.100.20"
  interfaces:
    - eth0
- service_name: remote-service
  location: !WireGuard
    endpoint: "remote.example.com"
  wireguard_ip: "10.42.0.10"
  wireguard_public_key: "test-public-key"
  interfaces:
    - wg0
"#;

        let services: Vec<ServiceNetwork> =
            serde_yaml::from_str(services_yaml).expect("Failed to parse services YAML");

        for service in services {
            network_manager
                .register_service(service)
                .await
                .expect("Failed to register service");
        }

        // Verify WireGuard is needed
        assert!(network_manager.requires_wireguard());
        assert_eq!(network_manager.services_requiring_wireguard().len(), 1);

        // Test resolution to WireGuard service
        let ip = network_manager
            .resolve_service_ip("local-service", "remote-service")
            .expect("Failed to resolve IP");
        assert_eq!(ip.to_string(), "10.42.0.10");

        // Test resolution from LAN to WireGuard
        let ip = network_manager
            .resolve_service_ip("lan-service", "remote-service")
            .expect("Failed to resolve IP");
        assert_eq!(ip.to_string(), "10.42.0.10");

        // Test local to LAN resolution
        let ip = network_manager
            .resolve_service_ip("local-service", "lan-service")
            .expect("Failed to resolve IP");
        assert_eq!(ip.to_string(), "192.168.100.20");

        // Test environment generation includes correct IPs
        let env = network_manager
            .generate_environment("local-service")
            .expect("Failed to generate environment");
        assert_eq!(
            env.get("LAN_SERVICE_ADDR"),
            Some(&"192.168.100.20".to_string())
        );
        assert_eq!(
            env.get("REMOTE_SERVICE_ADDR"),
            Some(&"10.42.0.10".to_string())
        );

        // Cleanup - stop compose stack in DinD container
        dind_compose_command("mixed-topology.yml", &project_name, vec!["down", "-v"])
            .await
            .expect("Failed to stop docker-compose in DinD");
    });
}

// Network discovery tests use shared Docker-in-Docker container with unique project names for isolation

#[test]
#[cfg(feature = "docker-tests")]
fn test_dind_container_setup() {
    if !smol::block_on(check_docker()) {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    smol::block_on(async {
        // Just test that we can start the DinD container
        ensure_dind_container_running()
            .await
            .expect("Failed to ensure DinD container is running");
    });
}

#[test]
#[cfg(feature = "docker-tests")]
fn test_ip_allocation_with_network_manager() {
    smol::block_on(async {
        let config = NetworkConfig::default();
        let mut network_manager =
            NetworkManager::new(config).expect("Failed to create network manager");

        // Register a service that needs WireGuard IP allocation
        network_manager
            .register_service(ServiceNetwork {
                service_name: "needs-allocation".to_string(),
                location: NetworkLocation::WireGuard {
                    endpoint: "needs-ip.example.com".to_string(),
                },
                host_ip: None,
                lan_ip: None,
                wireguard_ip: None, // No IP yet
                wireguard_public_key: Some("test-key".to_string()),
                interfaces: vec![],
            })
            .await
            .expect("Failed to register service");

        // In a real implementation, the IP would be allocated
        // For now, we just verify the service was registered
        assert!(network_manager.requires_wireguard());
    });
}
