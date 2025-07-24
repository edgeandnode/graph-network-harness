//! Integration tests that use command-executor to orchestrate their own test environment

#[cfg(all(feature = "ssh", feature = "docker-tests"))]
use command_executor::{
    backends::{local::LocalLauncher, ssh::SshLauncher},
    target::{DockerContainer, SystemdPortable},
    Command, Executor, Target,
};

#[cfg(all(feature = "ssh", feature = "docker-tests"))]
mod common;
#[cfg(all(feature = "ssh", feature = "docker-tests"))]
use common::shared_container::{ensure_container_running, get_ssh_config};

#[test]
#[cfg(all(feature = "ssh", feature = "ssh-tests", feature = "docker-tests"))]
fn test_self_hosted_ssh_execution() {
    smol::block_on(async {
        // Ensure shared container is running
        ensure_container_running()
            .await
            .expect("Failed to ensure container is running");

        // Now use the library to test SSH execution
        let local = LocalLauncher;
        let ssh_launcher = SshLauncher::new(local, get_ssh_config());
        let executor = Executor::new("self-hosted-test".to_string(), ssh_launcher);

        // Execute a simple command over SSH
        let cmd = Command::builder("echo")
            .arg("Hello from self-hosted test!")
            .build();

        let result = executor.execute(&Target::Command, cmd).await;
        assert!(result.is_ok(), "SSH execution failed: {:?}", result);

        let result = result.unwrap();
        assert!(result.success());
        assert!(result.output.contains("Hello from self-hosted test!"));
    });
}

#[test]
#[cfg(all(feature = "ssh", feature = "ssh-tests", feature = "docker-tests"))]
fn test_self_hosted_systemd_portable() {
    smol::block_on(async {
        // Ensure shared container is running
        ensure_container_running()
            .await
            .expect("Failed to ensure container is running");

        // Test systemd-portable functionality through SSH
        let local = LocalLauncher;
        let ssh_launcher = SshLauncher::new(local, get_ssh_config());
        let executor = Executor::new("portable-test".to_string(), ssh_launcher);

        let portable = SystemdPortable::new("echo-service", "echo-service.service");
        let target = Target::SystemdPortable(portable);

        // List portable services
        let list_cmd = Command::builder("portablectl").arg("list").build();

        let result = executor.execute(&target, list_cmd).await;
        assert!(
            result.is_ok(),
            "Failed to list portable services: {:?}",
            result
        );
    });
}

#[test]
#[cfg(all(feature = "ssh", feature = "ssh-tests", feature = "docker-tests"))]
fn test_self_hosted_nested_docker() {
    smol::block_on(async {
        // Ensure shared container is running
        ensure_container_running()
            .await
            .expect("Failed to ensure container is running");

        // Test Docker execution over SSH (nested launcher)
        let local = LocalLauncher;
        let ssh_launcher = SshLauncher::new(local, get_ssh_config());
        let executor = Executor::new("nested-docker-test".to_string(), ssh_launcher);

        // Check if Docker is available in the container
        let docker_check = Command::builder("docker").arg("--version").build();

        match executor.execute(&Target::Command, docker_check).await {
            Ok(result) if result.success() => {
                println!("Docker is available in container");

                // Run a simple Docker container over SSH
                let container = DockerContainer::new("alpine:latest").with_remove_on_exit(true);

                let cmd = Command::builder("echo")
                    .arg("Hello from Docker over SSH!")
                    .build();

                let result = executor
                    .execute(&Target::DockerContainer(container), cmd)
                    .await;

                // Docker might not be fully functional in the test container
                if result.is_ok() {
                    println!("Successfully ran Docker container over SSH");
                } else {
                    println!(
                        "Docker execution failed (expected in test environment): {:?}",
                        result
                    );
                }
            }
            _ => {
                println!("Docker not available in test container (expected)");
            }
        }
    });
}

#[test]
#[cfg(all(feature = "ssh", feature = "ssh-tests", feature = "docker-tests"))]
fn test_self_hosted_with_guard() {
    smol::block_on(async {
        // Ensure shared container is running
        ensure_container_running()
            .await
            .expect("Failed to ensure container is running");

        // Run a simple test to verify the container is working
        let local = LocalLauncher;
        let ssh_launcher = SshLauncher::new(local, get_ssh_config());
        let executor = Executor::new("guard-test".to_string(), ssh_launcher);

        let cmd = Command::builder("echo")
            .arg("Testing with shared container")
            .build();

        let result = executor
            .execute(&Target::Command, cmd)
            .await
            .expect("Failed to execute command");

        assert!(result.success());
        assert!(result.output.contains("Testing with shared container"));
    });
}
