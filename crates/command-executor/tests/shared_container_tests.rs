//! Tests that verify the shared container pattern works correctly
//!
//! These tests demonstrate that multiple tests can share the same container instance

#![cfg(all(feature = "ssh", feature = "ssh-tests", feature = "docker-tests"))]

use command_executor::{
    backends::{local::LocalLauncher, ssh::SshLauncher},
    Command, Executor, Target,
};

mod common;

// Import the shared container functionality from integration_nested.rs
// This is the same shared harness used by other SSH tests
use crate::common::shared_container::{ensure_container_running, get_ssh_config};

#[smol_potat::test]
async fn test_shared_container_first_access() {
    // This might be the first test to run - ensure container is ready
    ensure_container_running()
        .await
        .expect("Failed to ensure container is running");

    // Test SSH connection
    let local = LocalLauncher;
    let ssh_launcher = SshLauncher::new(local, get_ssh_config());
    let executor = Executor::new("test-first".to_string(), ssh_launcher);

    // Create a marker file to show this test ran
    let cmd = Command::builder("sh")
        .arg("-c")
        .arg("echo 'first test ran' > /tmp/shared_test_1.txt")
        .build();

    match executor.execute(&Target::Command, cmd).await {
        Ok(result) => {
            if !result.success() {
                println!("Command failed with output: {}", result.output);
                println!("Exit status: {:?}", result.status);
            }
            assert!(result.success());
        }
        Err(e) => {
            panic!("Failed to execute command: {:?}", e);
        }
    }
}

#[smol_potat::test]
async fn test_shared_container_second_access() {
    // Container should already be running from first test
    ensure_container_running()
        .await
        .expect("Failed to ensure container is running");

    // Test that we can see the file from the first test
    let local = LocalLauncher;
    let ssh_launcher = SshLauncher::new(local, get_ssh_config());
    let executor = Executor::new("test-second".to_string(), ssh_launcher);

    // Check if the first test's file exists
    let cmd = Command::builder("cat")
        .arg("/tmp/shared_test_1.txt")
        .build();

    let result = executor
        .execute(&Target::Command, cmd)
        .await
        .expect("Failed to execute command");

    // This proves the container is shared - we can see the first test's file
    assert!(result.success());
    assert!(result.output.contains("first test ran"));

    // Create our own marker
    let cmd2 = Command::builder("sh")
        .arg("-c")
        .arg("echo 'second test ran' > /tmp/shared_test_2.txt")
        .build();

    let result2 = executor
        .execute(&Target::Command, cmd2)
        .await
        .expect("Failed to execute command");
    assert!(result2.success());
}

#[smol_potat::test]
async fn test_shared_container_third_access() {
    // Container should still be running
    ensure_container_running()
        .await
        .expect("Failed to ensure container is running");

    let local = LocalLauncher;
    let ssh_launcher = SshLauncher::new(local, get_ssh_config());
    let executor = Executor::new("test-third".to_string(), ssh_launcher);

    // Check that both previous test files exist
    let cmd = Command::builder("sh")
        .arg("-c")
        .arg("ls -la /tmp/shared_test_*.txt")
        .build();

    let result = executor
        .execute(&Target::Command, cmd)
        .await
        .expect("Failed to list files");

    assert!(result.success());
    // Should see both files from previous tests
    assert!(result.output.contains("shared_test_1.txt"));
    assert!(result.output.contains("shared_test_2.txt"));
}

#[smol_potat::test]
async fn test_shared_container_verify_sharing() {
    // Just verify the container is shared by checking uptime
    ensure_container_running()
        .await
        .expect("Failed to ensure container is running");

    let local = LocalLauncher;
    let ssh_launcher = SshLauncher::new(local, get_ssh_config());
    let executor = Executor::new("test-verify".to_string(), ssh_launcher);

    // Get container uptime - if it's shared, it should have been up for a while
    let cmd = Command::builder("uptime").arg("-s").build();

    let result = executor
        .execute(&Target::Command, cmd)
        .await
        .expect("Failed to get uptime");

    assert!(result.success());
    println!("Container has been up since: {}", result.output.trim());

    // The fact that all these tests pass with files persisting between them
    // proves the container is truly shared
}
