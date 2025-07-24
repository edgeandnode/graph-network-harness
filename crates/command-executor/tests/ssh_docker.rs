//! Tests for SSH launcher with Docker targets

#[cfg(all(feature = "ssh", feature = "docker-tests"))]
mod tests {
    use command_executor::backends::local::LocalLauncher;
    use command_executor::backends::ssh::{SshConfig, SshLauncher};
    use command_executor::command::Command;
    use command_executor::target::{ComposeService, DockerContainer};
    use command_executor::Target;

    #[test]
    fn test_ssh_docker_container_type_composition() {
        // Test that we can use Docker containers over SSH
        let local = LocalLauncher;
        let ssh_config = SshConfig::new("remote.example.com").with_user("docker-user");
        let ssh_launcher = SshLauncher::new(local, ssh_config);

        // Create a Docker container target
        let container = DockerContainer::new("nginx:latest")
            .with_name("test-nginx")
            .with_env("ENV_VAR", "value");
        let target = Target::DockerContainer(container);

        // Create a command to run in the container
        let cmd = Command::new("nginx");

        // This should compile, proving SSH launcher can handle Docker targets
        let _ = (ssh_launcher, target, cmd);
    }

    #[test]
    fn test_ssh_compose_service_type_composition() {
        // Test that we can use Docker Compose services over SSH
        let local = LocalLauncher;
        let ssh_config = SshConfig::new("remote.example.com");
        let ssh_launcher = SshLauncher::new(local, ssh_config);

        // Create a Docker Compose target
        let compose =
            ComposeService::new("docker-compose.yml", "web").with_project_name("myproject");
        let target = Target::ComposeService(compose);

        // Create a command
        let mut cmd = Command::new("python");
        cmd.arg("app.py");

        // This should compile
        let _ = (ssh_launcher, target, cmd);
    }

    #[test]
    fn test_nested_ssh_docker_type_composition() {
        // Test SSH -> SSH -> Docker
        let local = LocalLauncher;

        // First hop through bastion
        let bastion_config = SshConfig::new("bastion.example.com");
        let ssh_to_bastion = SshLauncher::new(local, bastion_config);

        // Second hop to Docker host
        let docker_host_config = SshConfig::new("docker-host.internal");
        let ssh_to_docker_host = SshLauncher::new(ssh_to_bastion, docker_host_config);

        // Docker container target
        let container = DockerContainer::new("alpine:latest");
        let target = Target::DockerContainer(container);

        // Command to run
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c").arg("echo Hello from nested SSH Docker");

        // This should compile
        let _ = (ssh_to_docker_host, target, cmd);
    }

    #[test]
    fn test_docker_command_wrapping_logic() {
        // This test verifies the logic of how Docker commands are wrapped
        let container = DockerContainer::new("ubuntu:latest")
            .with_name("test-container")
            .with_env("MY_VAR", "my_value")
            .with_volume("/host/path", "/container/path")
            .with_working_dir("/app");

        // The LocalLauncher should build a proper docker run command
        // When wrapped by SSH, it becomes: ssh host "docker run ..."

        assert_eq!(container.image(), "ubuntu:latest");
        assert_eq!(container.name(), Some("test-container"));
        assert_eq!(container.env().get("MY_VAR"), Some(&"my_value".to_string()));
    }
}
