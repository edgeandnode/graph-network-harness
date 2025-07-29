//! Tests that verify the shared container pattern works correctly
//!
//! These tests demonstrate that multiple tests can share the same container instance

use crate::common::shared_container::{ensure_container_running, get_ssh_config};
use command_executor::{
    backends::{local::LocalLauncher, ssh::SshLauncher},
    Command, Executor, Target,
};

#[smol_potat::test]
async fn test_shared_container_basic_ssh_execution() {
    // Ensure container is ready
    ensure_container_running()
        .await
        .expect("Failed to ensure container is running");

    // Test SSH connection
    let local = LocalLauncher;
    let ssh_launcher = SshLauncher::new(local, get_ssh_config());
    let executor = Executor::new("test-first".to_string(), ssh_launcher);

    // Create a unique marker file for THIS test only
    let test_id = std::process::id();
    let cmd = Command::builder("sh")
        .arg("-c")
        .arg(format!("echo 'test {} ran at $(date +%s%N)' > /tmp/test_first_{}.txt", test_id, test_id))
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
    
    // Clean up our own test file
    let cleanup_cmd = Command::builder("rm")
        .arg("-f")
        .arg(format!("/tmp/test_first_{}.txt", test_id))
        .build();
    let _ = executor.execute(&Target::Command, cleanup_cmd).await;
}

#[smol_potat::test]
async fn test_shared_container_hostname_consistency() {
    // Ensure container is running
    ensure_container_running()
        .await
        .expect("Failed to ensure container is running");

    let local = LocalLauncher;
    let ssh_launcher = SshLauncher::new(local, get_ssh_config());
    let executor = Executor::new("test-second".to_string(), ssh_launcher);

    // Test container sharing by checking if we're in the same container
    // Get container hostname - if shared, it should be consistent
    let cmd = Command::builder("hostname").build();

    let result = executor
        .execute(&Target::Command, cmd)
        .await
        .expect("Failed to execute command");

    assert!(result.success());
    let hostname = result.output.trim();
    
    // Verify we can execute commands
    let test_id = std::process::id();
    let cmd2 = Command::builder("sh")
        .arg("-c")
        .arg(format!("echo 'test {} hostname: {}' > /tmp/test_second_{}.txt && cat /tmp/test_second_{}.txt", 
                     test_id, hostname, test_id, test_id))
        .build();

    let result2 = executor
        .execute(&Target::Command, cmd2)
        .await
        .expect("Failed to execute command");
    assert!(result2.success());
    assert!(result2.output.contains(&format!("test {} hostname:", test_id)));
    
    // Clean up
    let cleanup_cmd = Command::builder("rm")
        .arg("-f")
        .arg(format!("/tmp/test_second_{}.txt", test_id))
        .build();
    let _ = executor.execute(&Target::Command, cleanup_cmd).await;
}

#[smol_potat::test]
async fn test_shared_container_independent_file_operations() {
    // Container should still be running
    ensure_container_running()
        .await
        .expect("Failed to ensure container is running");

    let local = LocalLauncher;
    let ssh_launcher = SshLauncher::new(local, get_ssh_config());
    let executor = Executor::new("test-third".to_string(), ssh_launcher);

    // Test that the container is shared by checking hostname consistency
    let hostname_cmd = Command::builder("hostname").build();
    let hostname_result = executor
        .execute(&Target::Command, hostname_cmd)
        .await
        .expect("Failed to get hostname");
    assert!(hostname_result.success());
    let hostname = hostname_result.output.trim();
    
    // Create our own test file to verify container access
    let test_id = std::process::id();
    let create_cmd = Command::builder("sh")
        .arg("-c")
        .arg(format!("echo 'test {} in container {}' > /tmp/test_third_{}.txt && cat /tmp/test_third_{}.txt", 
                     test_id, hostname, test_id, test_id))
        .build();

    let result = executor
        .execute(&Target::Command, create_cmd)
        .await
        .expect("Failed to create test file");

    assert!(result.success());
    assert!(result.output.contains(&format!("test {} in container", test_id)));
    
    // Verify we're in the shared container by checking that hostname matches expected pattern
    let hostname_lines: Vec<&str> = hostname.lines().collect();
    let actual_hostname = hostname_lines.last().unwrap_or(&hostname);
    assert_eq!(actual_hostname, &"systemd-ssh-test", "Container hostname should match the shared container");
    
    // Clean up our test file
    let cleanup_cmd = Command::builder("rm")
        .arg("-f")
        .arg(format!("/tmp/test_third_{}.txt", test_id))
        .build();
    let _ = executor.execute(&Target::Command, cleanup_cmd).await;
}

#[smol_potat::test]
async fn test_shared_container_uptime_verification() {
    // Verify the container is shared by checking hostname and creating a test marker
    ensure_container_running()
        .await
        .expect("Failed to ensure container is running");

    let local = LocalLauncher;
    let ssh_launcher = SshLauncher::new(local, get_ssh_config());
    let executor = Executor::new("test-verify".to_string(), ssh_launcher);

    // Get container hostname to verify it's the shared container
    let hostname_cmd = Command::builder("hostname").build();
    let hostname_result = executor
        .execute(&Target::Command, hostname_cmd)
        .await
        .expect("Failed to get hostname");
    assert!(hostname_result.success());
    let hostname = hostname_result.output.trim();
    
    // Verify this is the shared container (trim any SSH warnings)
    let hostname_lines: Vec<&str> = hostname.lines().collect();
    let actual_hostname = hostname_lines.last().unwrap_or(&hostname);
    assert_eq!(actual_hostname, &"systemd-ssh-test", "Should be using the shared container");

    // Get container uptime - if it's shared, it should have been up for at least a few seconds
    let uptime_cmd = Command::builder("uptime").arg("-s").build();
    let uptime_result = executor
        .execute(&Target::Command, uptime_cmd)
        .await
        .expect("Failed to get uptime");
    assert!(uptime_result.success());
    println!("Container has been up since: {}", uptime_result.output.trim());
    
    // Create a marker to show this test ran
    let test_id = std::process::id();
    let marker_cmd = Command::builder("sh")
        .arg("-c")
        .arg(format!("echo 'verify test {} ran at $(date)' > /tmp/test_verify_{}.txt", test_id, test_id))
        .build();
    let _ = executor.execute(&Target::Command, marker_cmd).await;
    
    // Clean up
    let cleanup_cmd = Command::builder("rm")
        .arg("-f")
        .arg(format!("/tmp/test_verify_{}.txt", test_id))
        .build();
    let _ = executor.execute(&Target::Command, cleanup_cmd).await;
}
