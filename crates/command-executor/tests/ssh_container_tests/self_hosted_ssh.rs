//! SSH tests from self_hosted_integration.rs

use crate::common::shared_container::{ensure_container_running, get_ssh_config};
use command_executor::{
    Command, Executor, Target,
    backends::{local::LocalLauncher, ssh::SshLauncher},
};

#[smol_potat::test]
async fn test_self_hosted_ssh_execution() {
    // Ensure container is running
    ensure_container_running()
        .await
        .expect("Failed to ensure container is running");

    let local = LocalLauncher;
    let ssh_launcher = SshLauncher::new(local, get_ssh_config());
    let executor = Executor::new("self-hosted-ssh".to_string(), ssh_launcher);

    // Test basic command execution
    let cmd = Command::builder("echo").arg("Self-hosted SSH test").build();
    let result = executor
        .execute(&Target::Command, cmd)
        .await
        .expect("Failed to execute command");

    assert!(result.success());
    assert!(result.output.contains("Self-hosted SSH test"));
}
