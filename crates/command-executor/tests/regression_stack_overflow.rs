//! tRegression test for stack overflow issue with SSH launcher
//!
//! This test helps debug the stack overflow that occurs in test_portable_attach_detach_via_ssh

use command_executor::error::Error;

#[cfg(feature = "ssh")]
use command_executor::backends::LocalLauncher;
// TODO: SSH functionality moved to layered system - these tests need updating
// use command_executor::layered::{LayeredExecutor, SshLayer};
#[cfg(feature = "ssh")]
use command_executor::{Command, Executor, Target};

mod common;

// TODO: SSH functionality moved to layered system - this test needs rewriting
/*
#[cfg(feature = "ssh")]
#[smol_potat::test]
async fn test_simple_ssh_localhost_command() {
    // Create SSH launcher targeting localhost
    let local = LocalLauncher;
    let ssh_config = command_executor::backends::ssh::SshConfig::new("localhost")
        .with_extra_arg("-o")
        .with_extra_arg("StrictHostKeyChecking=no")
        .with_extra_arg("-o")
        .with_extra_arg("UserKnownHostsFile=/dev/null");

    let ssh_launcher = SshLauncher::new(local, ssh_config);
    let executor = Executor::new("test-ssh-localhost".to_string(), ssh_launcher);

    // Simple echo command
    let cmd = Command::builder("echo").arg("hello").build();
    let target = Target::Command;

    println!("About to execute command via SSH to localhost...");
    let result = executor.execute(&target, cmd).await;

    // This might fail if SSH is not set up, but should not stack overflow
    println!("Result: {:?}", result);
    assert!(
        result.is_ok() || result.is_err(),
        "Should either succeed or fail, not stack overflow"
    );
}
*/

// TODO: SSH functionality moved to layered system - this test needs rewriting
/*
#[cfg(feature = "ssh")]
#[smol_potat::test]
async fn test_ssh_with_sudo_command() {
    // This mimics what the failing test does
    let local = LocalLauncher;
    let ssh_config = command_executor::backends::ssh::SshConfig::new("localhost")
        .with_extra_arg("-o")
        .with_extra_arg("StrictHostKeyChecking=no")
        .with_extra_arg("-o")
        .with_extra_arg("UserKnownHostsFile=/dev/null");

    let ssh_launcher = SshLauncher::new(local, ssh_config);
    let executor = Executor::new("test-ssh-sudo".to_string(), ssh_launcher);

    // Command with sudo like in the failing test
    let cmd = Command::builder("sudo")
        .arg("-n")
        .arg("echo")
        .arg("hello")
        .build();
    let target = Target::Command;

    println!("About to execute sudo command via SSH...");
    let result = executor.execute(&target, cmd).await;
    println!("Result: {:?}", result);

    // This might fail due to sudo, but should not stack overflow
    assert!(
        result.is_ok() || result.is_err(),
        "Should either succeed or fail, not stack overflow"
    );
}
*/

// SSH container tests moved to ssh_container_tests/regression_ssh.rs

#[test]
fn test_error_debug_formatting() {
    // Test that deeply nested errors don't cause stack overflow when formatted
    let mut error = Error::spawn_failed("base error");

    // Create a deeply nested error chain
    for i in 0..1000 {
        error = error.with_layer_context(format!("Layer{}", i));
    }

    // This should not cause a stack overflow
    let formatted = format!("{:?}", error);
    println!(
        "Formatted error (truncated): {}...",
        &formatted[..formatted.len().min(100)]
    );

    // Also test Display formatting
    let display = format!("{}", error);
    println!("Display error: {}", display);
}
