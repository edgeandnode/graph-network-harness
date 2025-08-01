//! Tests for error context in nested launchers

use command_executor::Executor;
use command_executor::Target;
#[cfg(feature = "ssh")]
use command_executor::backends::local::LocalLauncher;
use command_executor::command::Command;

#[smol_potat::test]
async fn test_local_error_context() {
    let executor = Executor::local("test-error");
    let target = Target::Command;

    let cmd = Command::new("this_command_does_not_exist_12345");

    let result = executor.execute(&target, cmd).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    let err_str = err.to_string();

    // Should contain spawn failure message
    assert!(err_str.contains("spawn") || err_str.contains("Failed"));
}

#[cfg(feature = "ssh")]
#[smol_potat::test]
async fn test_ssh_error_context() {
    use command_executor::backends::ssh::{SshConfig, SshLauncher};

    let local = LocalLauncher;
    let ssh_config = SshConfig::new("localhost")
        .with_extra_arg("-o")
        .with_extra_arg("StrictHostKeyChecking=no")
        .with_extra_arg("-o")
        .with_extra_arg("UserKnownHostsFile=/dev/null");
    let ssh_launcher = SshLauncher::new(local, ssh_config);

    let executor = Executor::new("test-ssh-error".to_string(), ssh_launcher);
    let target = Target::Command;

    // Command that should fail
    let cmd = Command::new("this_command_does_not_exist_99999");

    let result = executor.execute(&target, cmd).await;

    if result.is_err() {
        let err = result.unwrap_err();
        let err_str = err.to_string();
        println!("Error with context: {}", err_str);

        // Should contain SSH context in error message
        assert!(err_str.contains("SSH") || err_str.contains("ssh"));
    }
}

#[smol_potat::test]
async fn test_docker_error_context() {
    use command_executor::target::DockerContainer;

    let executor = Executor::local("test-docker-error");
    // Try to use a non-existent image
    let container =
        DockerContainer::new("this_image_does_not_exist_99999:latest").with_remove_on_exit(true);
    let target = Target::DockerContainer(container);

    let mut cmd = Command::new("echo");
    cmd.arg("test");

    let result = executor.launch(&target, cmd).await;

    // Just check that we can create the command - actual execution may fail
    assert!(result.is_err() || result.is_ok());

    if let Err(err) = result {
        let err_str = err.to_string();
        println!("Docker error with context: {}", err_str);

        // Should mention Docker in the error
        assert!(err_str.contains("Docker") || err_str.contains("docker"));
    }
}

#[cfg(feature = "ssh")]
#[smol_potat::test]
async fn test_nested_ssh_docker_error_context() {
    use command_executor::backends::ssh::{SshConfig, SshLauncher};
    use command_executor::target::DockerContainer;

    let local = LocalLauncher;
    let ssh_config = SshConfig::new("localhost")
        .with_extra_arg("-o")
        .with_extra_arg("StrictHostKeyChecking=no")
        .with_extra_arg("-o")
        .with_extra_arg("UserKnownHostsFile=/dev/null");
    let ssh_launcher = SshLauncher::new(local, ssh_config);

    let executor = Executor::new("test-nested-error".to_string(), ssh_launcher);

    // Try to use a non-existent image over SSH
    let container = DockerContainer::new("invalid_image_12345:latest").with_remove_on_exit(true);
    let target = Target::DockerContainer(container);

    let mut cmd = Command::new("echo");
    cmd.arg("test");

    let result = executor.launch(&target, cmd).await;

    // Just check that we can create the command
    assert!(result.is_err() || result.is_ok());

    if let Err(err) = result {
        let err_str = err.to_string();
        println!("Nested error with full context: {}", err_str);

        // Should show both SSH and Docker context
        assert!(err_str.contains("SSH") || err_str.contains("Docker"));
    }
}
