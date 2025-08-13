//! Tests for error context in nested launchers

use command_executor::Executor;
use command_executor::Target;
// TODO: SSH functionality moved to layered system
// #[cfg(feature = "ssh")]
// use command_executor::backends::LocalLauncher;
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

// TODO: SSH functionality moved to layered system - this test needs rewriting
/*
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
*/

// Docker container target has been removed - use layered executor or 
// service-orchestration's ServiceTarget::Docker instead

// SSH and Docker functionality moved to layered system - use LayeredExecutor instead
