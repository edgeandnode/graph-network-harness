//! SSH tests from regression_stack_overflow.rs

use crate::common::shared_container::{ensure_container_running, get_ssh_config};
use command_executor::{
    backends::{local::LocalLauncher, ssh::SshLauncher},
    Command, Executor, ProcessHandle, Target,
};
use futures::StreamExt;

#[smol_potat::test]
async fn test_ssh_to_container() {
    // This test tries SSH to the actual container
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

#[smol_potat::test]
async fn test_ssh_launch_method() {
    // Test the launch method specifically
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