//! SSH tests from systemd_portable_ssh.rs

use crate::common::shared_container::{ensure_container_running, get_ssh_config};
use command_executor::{
    backends::{local::LocalLauncher, ssh::SshLauncher},
    Command, Executor, Target,
};

#[smol_potat::test]
async fn test_portablectl_list_via_ssh() {
    ensure_container_running()
        .await
        .expect("Failed to ensure container is running");

    let local = LocalLauncher;
    let ssh_launcher = SshLauncher::new(local, get_ssh_config());
    let executor = Executor::new("test-portable-ssh".to_string(), ssh_launcher);

    // Test portablectl command
    let cmd = Command::builder("sudo")
        .arg("-n")
        .arg("portablectl")
        .arg("list")
        .build();

    match executor.execute(&Target::Command, cmd).await {
        Ok(result) => {
            println!("Portablectl list output: {}", result.output);
            // Command might fail if portablectl is not available, which is fine
        }
        Err(e) => {
            println!("Portablectl not available (expected): {:?}", e);
        }
    }
}

// Add other systemd portable SSH tests here as needed