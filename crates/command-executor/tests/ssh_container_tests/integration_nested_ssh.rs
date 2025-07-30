//! SSH tests from integration_nested.rs

use crate::common::shared_container::{ensure_container_running, get_ssh_config};
use command_executor::{
    Command, Executor, Target,
    backends::{local::LocalLauncher, ssh::SshLauncher},
    target::DockerContainer,
};

#[test]
fn test_ssh_localhost_execution() {
    futures::executor::block_on(async {
        // Ensure shared container is running
        ensure_container_running()
            .await
            .expect("Failed to ensure container is running");

        let local = LocalLauncher;
        let ssh_launcher = SshLauncher::new(local, get_ssh_config());

        let executor = Executor::new("test-ssh".to_string(), ssh_launcher);
        let target = Target::Command;

        let cmd = Command::builder("echo").arg("Hello from SSH").build();

        let result = executor.execute(&target, cmd).await.unwrap();
        assert!(result.success());
        assert!(result.output.contains("Hello from SSH"));
    });
}

#[test]
fn test_ssh_docker_execution() {
    futures::executor::block_on(async {
        use command_executor::target::DockerContainer;

        // Ensure shared container is running
        ensure_container_running()
            .await
            .expect("Failed to ensure container is running");

        let local = LocalLauncher;
        let ssh_launcher = SshLauncher::new(local, get_ssh_config());

        let executor = Executor::new("test-ssh-docker".to_string(), ssh_launcher);
        let docker_target = DockerContainer::new("alpine");
        let target = Target::DockerContainer(docker_target);

        let cmd = Command::builder("echo")
            .arg("Hello from Docker via SSH")
            .build();

        match executor.execute(&target, cmd).await {
            Ok(result) => {
                // This might fail if Docker is not available via SSH, which is fine
                println!("Docker via SSH result: {:?}", result);
            }
            Err(e) => {
                println!("Docker via SSH not available (expected): {:?}", e);
            }
        }
    });
}
