//! Regression test for stack overflow issue with SSH launcher
//! 
//! This test helps debug the stack overflow that occurs in test_portable_attach_detach_via_ssh

use command_executor::error::Error;

#[cfg(feature = "ssh")]
use command_executor::backends::{local::LocalLauncher, ssh::SshLauncher};
#[cfg(feature = "ssh")]
use command_executor::{Command, Executor, Target};

mod common;

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
    assert!(result.is_ok() || result.is_err(), "Should either succeed or fail, not stack overflow");
}

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
    assert!(result.is_ok() || result.is_err(), "Should either succeed or fail, not stack overflow");
}

#[cfg(all(feature = "ssh", feature = "docker-tests"))]
#[smol_potat::test]
async fn test_ssh_to_container() {
    // This test tries SSH to the actual container like the failing test
    use command_executor::ProcessHandle;
    use futures::StreamExt;
    use crate::common::shared_container::{ensure_container_running, get_ssh_config};
    
    ensure_container_running()
        .await
        .expect("Failed to ensure container is running");
    
    let local = LocalLauncher;
    let ssh_launcher = SshLauncher::new(local, get_ssh_config());
    let executor = Executor::new("test-ssh-container".to_string(), ssh_launcher);
    
    // Simple command first
    let cmd = Command::builder("echo").arg("test from container").build();
    let target = Target::Command;
    
    println!("About to execute command in container via SSH...");
    let result = executor.execute(&target, cmd).await;
    println!("Container command result: {:?}", result);
    
    assert!(result.is_ok(), "Basic SSH to container should work");
    assert!(result.unwrap().output.contains("test from container"));
}

#[cfg(all(feature = "ssh", feature = "docker-tests"))]
#[smol_potat::test]
async fn test_ssh_launch_method() {
    // Test the launch method specifically to see if the issue is in execute vs launch
    use command_executor::ProcessHandle;
    use futures::StreamExt;
    use crate::common::shared_container::{ensure_container_running, get_ssh_config};
    
    ensure_container_running()
        .await
        .expect("Failed to ensure container is running");
    
    let local = LocalLauncher;
    let ssh_launcher = SshLauncher::new(local, get_ssh_config());
    let executor = Executor::new("test-ssh-launch".to_string(), ssh_launcher);
    
    let cmd = Command::builder("echo").arg("testing launch").build();
    let target = Target::Command;
    
    println!("About to launch command...");
    let (mut events, mut handle) = executor.launch(&target, cmd).await
        .expect("Failed to launch");
    
    let mut output = String::new();
    println!("Reading events...");
    while let Some(event) = events.next().await {
        if let Some(data) = &event.data {
            println!("Event data: {}", data);
            output.push_str(data);
        }
    }
    
    let status = handle.wait().await.expect("Failed to wait");
    println!("Process exited with status: {:?}", status);
    
    assert!(output.contains("testing launch"));
}

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
    println!("Formatted error (truncated): {}...", &formatted[..formatted.len().min(100)]);
    
    // Also test Display formatting
    let display = format!("{}", error);
    println!("Display error: {}", display);
}