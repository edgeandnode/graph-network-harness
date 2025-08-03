//! Integration tests for the layered execution system.
//!
//! These tests demonstrate how layers compose to create complex execution pipelines
//! and verify that command transformation works correctly across different scenarios.

#[cfg(test)]
mod tests {
    use super::super::layers::ExecutionLayer;
    use super::super::{DockerLayer, ExecutionContext, LayeredExecutor, LocalLayer, SshLayer};
    use crate::{Command, backends::LocalLauncher};

    #[test]
    fn test_command_transformation_ssh_only() {
        let executor = LayeredExecutor::new(LocalLauncher)
            .with_layer(SshLayer::new("user@remote.example.com"));

        let mut command = Command::new("ls");
        command.arg("-la").arg("/tmp");

        let transformed = executor.transform_command_for_test(command).unwrap();

        // Should create: ssh user@remote.example.com "ls -la /tmp"
        assert_eq!(transformed.get_program(), "ssh");
        let args: Vec<&str> = transformed
            .get_args()
            .iter()
            .map(|s| s.to_str().unwrap())
            .collect();
        assert!(args.contains(&"user@remote.example.com"));
        assert!(args.iter().any(|&arg| arg.contains("ls -la /tmp")));
    }

    #[test]
    fn test_command_transformation_docker_only() {
        let executor =
            LayeredExecutor::new(LocalLauncher).with_layer(DockerLayer::new("my-container"));

        let mut command = Command::new("ps");
        command.arg("aux");

        let transformed = executor.transform_command_for_test(command).unwrap();

        // Should create: docker exec my-container sh -c "ps aux"
        assert_eq!(transformed.get_program(), "docker");
        let args: Vec<&str> = transformed
            .get_args()
            .iter()
            .map(|s| s.to_str().unwrap())
            .collect();
        assert!(args.contains(&"exec"));
        assert!(args.contains(&"my-container"));
        assert!(args.contains(&"sh"));
        assert!(args.contains(&"-c"));
        assert!(args.iter().any(|&arg| arg.contains("ps aux")));
    }

    #[test]
    fn test_multi_hop_ssh_transformation() {
        let executor = LayeredExecutor::new(LocalLauncher)
            .with_layer(SshLayer::new("user@jump-host"))
            .with_layer(SshLayer::new("user@target-host"));

        let command = Command::new("hostname");

        let final_command = executor.transform_command_for_test(command).unwrap();

        // Should create: ssh user@target-host "ssh user@jump-host hostname"
        assert_eq!(final_command.get_program(), "ssh");
        let args: Vec<&str> = final_command
            .get_args()
            .iter()
            .map(|s| s.to_str().unwrap())
            .collect();
        assert!(args.contains(&"user@target-host"));

        // The command string should contain the nested SSH call
        let command_arg = args
            .iter()
            .find(|&&arg| arg.contains("ssh user@jump-host"))
            .unwrap();
        assert!(command_arg.contains("hostname"));
    }

    #[test]
    fn test_ssh_then_docker_transformation() {
        let executor = LayeredExecutor::new(LocalLauncher)
            .with_layer(SshLayer::new("user@docker-host"))
            .with_layer(DockerLayer::new("app-container"));

        let mut command = Command::new("cat");
        command.arg("/app/config.json");

        let final_command = executor.transform_command_for_test(command).unwrap();

        // Layers are applied in the order added, so SSH is applied first, then Docker
        // This means the final command should be: docker exec app-container sh -c "ssh user@docker-host cat /app/config.json"
        assert_eq!(final_command.get_program(), "docker");
        let args: Vec<&str> = final_command
            .get_args()
            .iter()
            .map(|s| s.to_str().unwrap())
            .collect();
        assert!(args.contains(&"exec"));
        assert!(args.contains(&"app-container"));

        // Find the command argument that contains the ssh command
        let ssh_command = args
            .iter()
            .find(|&&arg| arg.contains("ssh user@docker-host"))
            .expect("Should contain ssh command");

        assert!(ssh_command.contains("cat /app/config.json"));
    }

    #[test]
    fn test_complex_three_layer_transformation() {
        let executor = LayeredExecutor::new(LocalLauncher)
            .with_layer(SshLayer::new("user@jump-server"))
            .with_layer(SshLayer::new("admin@target-server"))
            .with_layer(DockerLayer::new("database-container"));

        let mut command = Command::new("psql");
        command.arg("-c").arg("SELECT COUNT(*) FROM users;");

        let current_command = executor.transform_command_for_test(command).unwrap();

        // Layers applied in order: SSH(jump) -> SSH(target) -> Docker
        // Final command should be Docker exec with nested SSH commands
        assert_eq!(current_command.get_program(), "docker");
        let args: Vec<&str> = current_command
            .get_args()
            .iter()
            .map(|s| s.to_str().unwrap())
            .collect();
        assert!(args.contains(&"exec"));
        assert!(args.contains(&"database-container"));

        // The docker command should contain nested SSH calls
        let nested_command = args
            .iter()
            .find(|&&arg| arg.contains("ssh admin@target-server"))
            .expect("Should contain nested SSH command");

        assert!(nested_command.contains("ssh user@jump-server"));
        assert!(nested_command.contains("psql"));
        assert!(nested_command.contains("SELECT COUNT(*) FROM users"));
    }

    #[test]
    fn test_ssh_options_preservation() {
        let ssh_layer = SshLayer::new("user@secure-host")
            .with_port(2222)
            .with_identity_file("/path/to/key")
            .with_option("-o StrictHostKeyChecking=no");

        let executor = LayeredExecutor::new(LocalLauncher).with_layer(ssh_layer);

        let mut command = Command::new("uname");
        command.arg("-a");

        let transformed = executor.transform_command_for_test(command).unwrap();

        // Verify SSH options are included
        let args: Vec<&str> = transformed
            .get_args()
            .iter()
            .map(|s| s.to_str().unwrap())
            .collect();
        assert!(args.contains(&"-p"));
        assert!(args.contains(&"2222"));
        assert!(args.contains(&"-i"));
        assert!(args.contains(&"/path/to/key"));
        assert!(args.contains(&"-o StrictHostKeyChecking=no"));
        assert!(args.contains(&"user@secure-host"));
    }

    #[test]
    fn test_docker_options_preservation() {
        let docker_layer = DockerLayer::new("my-app")
            .with_interactive(true)
            .with_tty(true)
            .with_user("app-user")
            .with_working_dir("/app");

        let executor = LayeredExecutor::new(LocalLauncher).with_layer(docker_layer);

        let mut command = Command::new("npm");
        command.arg("start");

        let transformed = executor.transform_command_for_test(command).unwrap();

        // Verify Docker options are included
        let args: Vec<&str> = transformed
            .get_args()
            .iter()
            .map(|s| s.to_str().unwrap())
            .collect();
        assert!(args.contains(&"exec"));
        assert!(args.contains(&"-i"));
        assert!(args.contains(&"-t"));
        assert!(args.contains(&"-u"));
        assert!(args.contains(&"app-user"));
        assert!(args.contains(&"-w"));
        assert!(args.contains(&"/app"));
        assert!(args.contains(&"my-app"));
    }

    #[test]
    fn test_local_layer_environment_application() {
        let local_layer = LocalLayer::new();
        let context = ExecutionContext::new()
            .with_env("PATH", "/custom/bin:/usr/bin")
            .with_env("APP_ENV", "testing")
            .with_working_dir("/tmp/workspace");

        let command = Command::new("env");
        let transformed = local_layer.wrap_command(command, &context).unwrap();

        // Local layer should preserve the original command but apply environment
        assert_eq!(transformed.get_program(), "env");

        // Environment variables should be applied (we can't easily test this without
        // actually running the command, but we can verify the transformation succeeds)
    }

    #[test]
    fn test_shell_escaping_with_special_characters() {
        let executor = LayeredExecutor::new(LocalLauncher).with_layer(SshLayer::new("user@host"));

        let mut command = Command::new("echo");
        command
            .arg("Hello World!")
            .arg("$HOME/test")
            .arg("file with spaces.txt")
            .arg("'quoted string'");

        let transformed = executor.transform_command_for_test(command).unwrap();

        // Find the command string argument
        let args: Vec<&str> = transformed
            .get_args()
            .iter()
            .map(|s| s.to_str().unwrap())
            .collect();
        let command_string = args
            .iter()
            .find(|&&arg| arg.contains("echo"))
            .expect("Should contain echo command");

        // Verify proper escaping of special characters
        assert!(command_string.contains("'Hello World!'"));
        assert!(command_string.contains("'$HOME/test'"));
        assert!(command_string.contains("'file with spaces.txt'"));
        // Single quotes should be escaped specially
        assert!(command_string.contains("'quoted string'"));
    }

    #[test]
    fn test_layer_composition_order() {
        // Verify that layers are applied in the order they're added
        let executor = LayeredExecutor::new(LocalLauncher)
            .with_layer(LocalLayer::new()) // Applied first (innermost)
            .with_layer(DockerLayer::new("app")) // Applied second (middle)
            .with_layer(SshLayer::new("user@host")); // Applied last (outermost)

        let mut command = Command::new("echo");
        command.arg("test");

        let transformed = executor.transform_command_for_test(command).unwrap();

        // The outermost command should be SSH
        assert_eq!(transformed.get_program(), "ssh");
        let args: Vec<&str> = transformed
            .get_args()
            .iter()
            .map(|s| s.to_str().unwrap())
            .collect();
        assert!(args.contains(&"user@host"));

        // The SSH command should contain a docker exec command
        let ssh_command = args
            .iter()
            .find(|&&arg| arg.contains("docker exec"))
            .expect("SSH command should wrap docker exec");

        assert!(ssh_command.contains("app"));
        assert!(ssh_command.contains("echo test"));
    }

    #[test]
    fn test_context_environment_propagation() {
        let executor = LayeredExecutor::new(LocalLauncher)
            .with_env("GLOBAL_VAR", "global_value")
            .with_layer(LocalLayer::new());

        let mut command = Command::new("printenv");
        command.arg("GLOBAL_VAR");

        // Test that the executor builds correctly with environment variables
        assert_eq!(
            executor.context().env.get("GLOBAL_VAR"),
            Some(&"global_value".to_string())
        );

        // Verify the command transforms correctly
        let transformed = executor.transform_command_for_test(command).unwrap();
        assert_eq!(transformed.get_program(), "printenv");
    }

    #[test]
    fn test_working_directory_propagation() {
        let executor = LayeredExecutor::new(LocalLauncher)
            .with_working_dir("/custom/workdir")
            .with_layer(LocalLayer::new());

        let command = Command::new("pwd");

        // Test that the executor builds correctly with working directory
        assert_eq!(
            executor.context().working_dir,
            Some("/custom/workdir".into())
        );

        // Verify the command transforms correctly
        let transformed = executor.transform_command_for_test(command).unwrap();
        assert_eq!(transformed.get_program(), "pwd");
    }

    #[test]
    fn test_empty_layer_stack() {
        let executor = LayeredExecutor::new(LocalLauncher);

        let mut command = Command::new("echo");
        command.arg("test");

        let transformed = executor.transform_command_for_test(command).unwrap();

        // With no layers, the command should be unchanged
        assert_eq!(transformed.get_program(), "echo");
        let args: Vec<&str> = transformed
            .get_args()
            .iter()
            .map(|s| s.to_str().unwrap())
            .collect();
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_ssh_layer_specific_environment() {
        let ssh_layer = SshLayer::new("user@remote")
            .with_env("REMOTE_VAR", "remote_value")
            .with_env("SSH_PATH", "/remote/bin:/usr/bin")
            .with_working_dir("/remote/workspace");

        let executor = LayeredExecutor::new(LocalLauncher).with_layer(ssh_layer);

        let mut command = Command::new("node");
        command.arg("app.js");

        let transformed = executor.transform_command_for_test(command).unwrap();

        // Should be SSH command
        assert_eq!(transformed.get_program(), "ssh");
        let args: Vec<&str> = transformed
            .get_args()
            .iter()
            .map(|s| s.to_str().unwrap())
            .collect();
        assert!(args.contains(&"user@remote"));

        // The remote command should include environment variables and working directory
        let command_arg = args
            .iter()
            .find(|&&arg| arg.contains("REMOTE_VAR=remote_value"))
            .expect("Should contain SSH layer environment variable");

        assert!(command_arg.contains("SSH_PATH=/remote/bin:/usr/bin"));
        assert!(command_arg.contains("cd /remote/workspace"));
        assert!(command_arg.contains("node app.js"));
    }

    #[test]
    fn test_docker_layer_specific_environment() {
        let docker_layer = DockerLayer::new("myapp")
            .with_env("API_KEY", "secret123")
            .with_env("DEBUG", "true")
            .with_user("app-user")
            .with_working_dir("/app");

        let executor = LayeredExecutor::new(LocalLauncher).with_layer(docker_layer);

        let mut command = Command::new("python");
        command.arg("script.py");

        let transformed = executor.transform_command_for_test(command).unwrap();

        // Should be Docker command
        assert_eq!(transformed.get_program(), "docker");
        let args: Vec<&str> = transformed
            .get_args()
            .iter()
            .map(|s| s.to_str().unwrap())
            .collect();
        assert!(args.contains(&"exec"));
        assert!(args.contains(&"myapp"));

        // Docker should have environment variables as -e flags
        assert!(args.contains(&"-e"));
        let has_api_key = args
            .windows(2)
            .any(|pair| pair[0] == "-e" && pair[1] == "API_KEY=secret123");
        let has_debug = args
            .windows(2)
            .any(|pair| pair[0] == "-e" && pair[1] == "DEBUG=true");
        assert!(
            has_api_key,
            "Should have API_KEY environment variable from layer"
        );
        assert!(
            has_debug,
            "Should have DEBUG environment variable from layer"
        );

        // Should have user and workdir from layer
        assert!(args.contains(&"-u"));
        assert!(args.contains(&"app-user"));
        assert!(args.contains(&"-w"));
        assert!(args.contains(&"/app"));

        // Should contain the python command
        let command_arg = args
            .iter()
            .find(|&&arg| arg.contains("python script.py"))
            .expect("Should contain python command");
        assert!(command_arg.contains("python script.py"));
    }

    #[test]
    fn test_working_directory_propagation_through_layers() {
        let executor = LayeredExecutor::new(LocalLauncher)
            .with_working_dir("/app/workspace")
            .with_layer(SshLayer::new("user@remote"))
            .with_layer(DockerLayer::new("container"));

        let mut command = Command::new("make");
        command.arg("build");

        let transformed = executor.transform_command_for_test(command).unwrap();

        // Final command should be Docker
        assert_eq!(transformed.get_program(), "docker");
        let args: Vec<&str> = transformed
            .get_args()
            .iter()
            .map(|s| s.to_str().unwrap())
            .collect();

        // Should contain the nested SSH command with make build
        let ssh_command = args
            .iter()
            .find(|&&arg| arg.contains("ssh user@remote"))
            .expect("Should contain SSH command");

        assert!(ssh_command.contains("make build"));
    }

    #[test]
    fn test_local_layer_specific_environment() {
        let local_layer = LocalLayer::new()
            .with_env("LOCAL_VAR", "local_value")
            .with_env("PATH", "/custom/bin:/usr/bin")
            .with_working_dir("/local/workspace");

        let executor = LayeredExecutor::new(LocalLauncher).with_layer(local_layer);

        let mut command = Command::new("cargo");
        command.arg("build");

        let transformed = executor.transform_command_for_test(command).unwrap();

        // Should still be cargo command (LocalLayer doesn't wrap)
        assert_eq!(transformed.get_program(), "cargo");
        let args: Vec<&str> = transformed
            .get_args()
            .iter()
            .map(|s| s.to_str().unwrap())
            .collect();
        assert!(args.contains(&"build"));

        // Environment variables are applied to the command but we can't easily test them
        // without actually running the command. The test verifies the layer builds correctly.
    }

    #[test]
    fn test_layer_specific_vs_global_environment_separation() {
        // This test demonstrates that each layer has its own environment
        // and the global ExecutionContext is separate
        let ssh_layer = SshLayer::new("user@remote").with_env("SSH_SPECIFIC", "ssh_value");

        let docker_layer =
            DockerLayer::new("container").with_env("DOCKER_SPECIFIC", "docker_value");

        let executor = LayeredExecutor::new(LocalLauncher)
            .with_env("GLOBAL_VAR", "global_value") // This should NOT appear in layers now
            .with_layer(ssh_layer)
            .with_layer(docker_layer);

        let mut command = Command::new("echo");
        command.arg("test");

        let transformed = executor.transform_command_for_test(command).unwrap();

        // Final command should be Docker (outermost layer)
        assert_eq!(transformed.get_program(), "docker");
        let args: Vec<&str> = transformed
            .get_args()
            .iter()
            .map(|s| s.to_str().unwrap())
            .collect();

        // Docker layer should have its own environment
        let has_docker_env = args
            .windows(2)
            .any(|pair| pair[0] == "-e" && pair[1] == "DOCKER_SPECIFIC=docker_value");
        assert!(
            has_docker_env,
            "Docker layer should have its specific environment"
        );

        // Should NOT have global environment in Docker layer
        let has_global_env = args
            .windows(2)
            .any(|pair| pair[0] == "-e" && pair[1] == "GLOBAL_VAR=global_value");
        assert!(
            !has_global_env,
            "Docker layer should NOT have global environment"
        );

        // SSH command should be nested inside
        let ssh_command = args
            .iter()
            .find(|&&arg| arg.contains("SSH_SPECIFIC=ssh_value"))
            .expect("Should contain SSH layer environment");

        assert!(ssh_command.contains("echo test"));
    }

    #[test]
    fn test_complex_multi_layer_with_different_environments() {
        // Create a complex pipeline: Local -> SSH -> Docker
        // Each layer has different environment variables
        let local_layer = LocalLayer::new()
            .with_env("LOCAL_PATH", "/local/bin")
            .with_working_dir("/local/workspace");

        let ssh_layer = SshLayer::new("deploy@production-server")
            .with_env("DEPLOY_ENV", "production")
            .with_env("SSH_KEY_PATH", "/home/deploy/.ssh/id_rsa")
            .with_working_dir("/opt/deployment");

        let docker_layer = DockerLayer::new("app-production")
            .with_env("APP_ENV", "production")
            .with_env("DATABASE_URL", "postgres://prod-db:5432/app")
            .with_user("appuser")
            .with_working_dir("/app");

        let executor = LayeredExecutor::new(LocalLauncher)
            .with_layer(local_layer) // Applied first (innermost)
            .with_layer(ssh_layer) // Applied second (middle)
            .with_layer(docker_layer); // Applied last (outermost)

        let mut command = Command::new("npm");
        command.arg("run").arg("migrate");

        let transformed = executor.transform_command_for_test(command).unwrap();

        // Outermost should be Docker
        assert_eq!(transformed.get_program(), "docker");
        let args: Vec<&str> = transformed
            .get_args()
            .iter()
            .map(|s| s.to_str().unwrap())
            .collect();

        // Verify Docker layer configuration
        assert!(args.contains(&"exec"));
        assert!(args.contains(&"app-production"));
        assert!(args.contains(&"-u"));
        assert!(args.contains(&"appuser"));
        assert!(args.contains(&"-w"));
        assert!(args.contains(&"/app"));

        // Verify Docker environment variables
        let has_app_env = args
            .windows(2)
            .any(|pair| pair[0] == "-e" && pair[1] == "APP_ENV=production");
        let has_db_url = args
            .windows(2)
            .any(|pair| pair[0] == "-e" && pair[1] == "DATABASE_URL=postgres://prod-db:5432/app");
        assert!(
            has_app_env && has_db_url,
            "Docker layer should have its environment"
        );

        // Verify nested SSH command with its environment
        let nested_ssh = args
            .iter()
            .find(|&&arg| arg.contains("DEPLOY_ENV=production"))
            .expect("Should contain SSH layer environment");

        assert!(nested_ssh.contains("SSH_KEY_PATH=/home/deploy/.ssh/id_rsa"));
        assert!(nested_ssh.contains("cd /opt/deployment"));
        assert!(nested_ssh.contains("deploy@production-server"));
        assert!(nested_ssh.contains("npm run migrate"));
    }

    #[test]
    fn test_ssh_credential_forwarding_options() {
        // Test SSH layer with credential forwarding and TTY options
        let secure_ssh = SshLayer::new("admin@secure-server")
            .with_agent_forwarding(true) // Enable SSH agent forwarding
            .with_x11_forwarding(true) // Enable X11 forwarding
            .with_tty(true) // Allocate TTY for interactive commands
            .with_port(2222)
            .with_identity_file("/home/admin/.ssh/secure_key")
            .with_env("SUDO_ASKPASS", "/usr/bin/ssh-askpass");

        let executor = LayeredExecutor::new(LocalLauncher).with_layer(secure_ssh);

        let mut command = Command::new("sudo");
        command
            .arg("-A")
            .arg("systemctl")
            .arg("status")
            .arg("secure-service");

        let transformed = executor.transform_command_for_test(command).unwrap();

        // Should be SSH command with forwarding flags
        assert_eq!(transformed.get_program(), "ssh");
        let args: Vec<&str> = transformed
            .get_args()
            .iter()
            .map(|s| s.to_str().unwrap())
            .collect();

        // Verify SSH forwarding flags are present
        assert!(args.contains(&"-A"), "Should have agent forwarding flag");
        assert!(args.contains(&"-X"), "Should have X11 forwarding flag");
        assert!(args.contains(&"-t"), "Should have TTY allocation flag");

        // Verify other SSH options
        assert!(args.contains(&"-p"));
        assert!(args.contains(&"2222"));
        assert!(args.contains(&"-i"));
        assert!(args.contains(&"/home/admin/.ssh/secure_key"));
        assert!(args.contains(&"admin@secure-server"));

        // Verify environment variable in remote command
        let remote_command = args.last().unwrap();
        assert!(remote_command.contains("SUDO_ASKPASS=/usr/bin/ssh-askpass"));
        assert!(remote_command.contains("sudo -A systemctl status secure-service"));
    }

    #[test]
    fn test_ssh_forwarding_with_docker_combination() {
        // Test SSH with forwarding combined with Docker
        let ssh_with_forwarding = SshLayer::new("devops@docker-host")
            .with_agent_forwarding(true) // Forward SSH agent for Git operations
            .with_env("GIT_SSH_COMMAND", "ssh -o StrictHostKeyChecking=no")
            .with_working_dir("/opt/docker-services");

        let docker_with_git = DockerLayer::new("build-container")
            .with_env("BUILD_ENV", "production")
            .with_working_dir("/workspace");

        let executor = LayeredExecutor::new(LocalLauncher)
            .with_layer(ssh_with_forwarding)
            .with_layer(docker_with_git);

        let mut command = Command::new("git");
        command
            .arg("clone")
            .arg("git@github.com:company/private-repo.git");

        let transformed = executor.transform_command_for_test(command).unwrap();

        // Should be Docker command (outermost)
        assert_eq!(transformed.get_program(), "docker");
        let args: Vec<&str> = transformed
            .get_args()
            .iter()
            .map(|s| s.to_str().unwrap())
            .collect();

        // Verify Docker environment
        let has_build_env = args
            .windows(2)
            .any(|pair| pair[0] == "-e" && pair[1] == "BUILD_ENV=production");
        assert!(has_build_env);

        // Verify nested SSH with agent forwarding
        let ssh_command = args
            .iter()
            .find(|&&arg| arg.contains("ssh") && arg.contains("-A"))
            .expect("Should contain SSH command with agent forwarding");

        assert!(ssh_command.contains("devops@docker-host"));
        assert!(ssh_command.contains("GIT_SSH_COMMAND"));
        assert!(ssh_command.contains("cd /opt/docker-services"));
        assert!(ssh_command.contains("git clone git@github.com:company/private-repo.git"));
    }
}
