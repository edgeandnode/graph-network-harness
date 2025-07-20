//! Tests for systemd-portable via SSH
//! 
//! These tests connect to the systemd container via SSH to test
//! systemd-portable functionality through our SSH launcher.

#![cfg(all(test, feature = "ssh-tests", feature = "docker-tests"))]

use command_executor::{Executor, Target, SystemdPortable, Command, ProcessHandle};
use command_executor::backends::{local::LocalLauncher, ssh::SshLauncher, sudo::SudoLauncher};
use futures::StreamExt;

mod common;
use common::shared_container::{ensure_container_running, get_ssh_config};

#[test]
fn test_portablectl_list_via_ssh() {
    futures::executor::block_on(async {
        ensure_container_running().await
            .expect("Failed to ensure container is running");
        
        let local = LocalLauncher;
        let ssh_launcher = SshLauncher::new(local, get_ssh_config());
        let executor = Executor::new("test-portable-ssh".to_string(), ssh_launcher);
        
        // SystemdPortable target
        let portable = SystemdPortable::new("echo-service", "echo-service.service");
        let target = Target::SystemdPortable(portable);
        
        // List portable services
        let list_cmd = Command::builder("portablectl")
            .arg("list")
            .build();
        
        let result = executor.execute(&target, list_cmd).await;
        assert!(result.is_ok(), "Failed to list portable services: {:?}", result);
    });
}

#[test]
fn test_portable_attach_detach_via_ssh() {
    futures::executor::block_on(async {
        ensure_container_running().await
            .expect("Failed to ensure container is running");
        
        // Create a nested launcher: SSH -> Sudo -> Local
        // This will SSH to the container and run commands with sudo there
        let local = LocalLauncher;
        let sudo_local = SudoLauncher::new(local);
        let ssh_sudo_launcher = SshLauncher::new(sudo_local, get_ssh_config());
        let executor = Executor::new("test-attach-ssh".to_string(), ssh_sudo_launcher);
        
        // Also create a regular SSH launcher for non-sudo commands
        let regular_ssh_launcher = SshLauncher::new(LocalLauncher, get_ssh_config());
        let regular_executor = Executor::new("test-attach-ssh-regular".to_string(), regular_ssh_launcher);
        
        // Use Target::Command for SSH execution
        let target = Target::Command;
        
        // First detach if already attached (will be wrapped with sudo automatically)
        let detach_cmd = Command::builder("portablectl")
            .arg("detach")
            .arg("echo-service")
            .build();
        
        // Ignore errors - might not be attached
        let _ = executor.execute(&target, detach_cmd).await;
        
        // Now attach (will be wrapped with sudo automatically)
        let attach_cmd = Command::builder("portablectl")
            .arg("attach")
            .arg("--copy=copy")
            .arg("/opt/portable-services/echo-service")
            .build();
        
        let attach_result = executor.execute(&target, attach_cmd).await;
        assert!(attach_result.is_ok(), "Failed to attach: {:?}", attach_result);
        
        // List to verify (doesn't need sudo)
        let list_cmd = Command::builder("portablectl")
            .arg("list")
            .build();
        
        let (mut events, mut handle) = regular_executor.launch(&target, list_cmd).await.unwrap();
        
        let mut found_echo = false;
        while let Some(event) = events.next().await {
            if let Some(data) = &event.data {
                if data.contains("echo-service") {
                    found_echo = true;
                    break;
                }
            }
        }
        
        let _ = handle.wait().await;
        assert!(found_echo, "echo-service should be listed after attach");
        
        // Clean up - detach (will be wrapped with sudo automatically)
        let detach_cmd = Command::builder("portablectl")
            .arg("detach")
            .arg("echo-service")
            .build();
        
        let detach_result = executor.execute(&target, detach_cmd).await;
        assert!(detach_result.is_ok(), "Failed to detach: {:?}", detach_result);
    });
}

#[test]
fn test_systemctl_via_ssh() {
    futures::executor::block_on(async {
        ensure_container_running().await
            .expect("Failed to ensure container is running");
        
        let local = LocalLauncher;
        let ssh_launcher = SshLauncher::new(local, get_ssh_config());
        let executor = Executor::new("test-systemctl-ssh".to_string(), ssh_launcher);
        
        // Regular command target for systemctl
        let target = Target::Command;
        
        // Check if SSH service is running (we know it is)
        let cmd = Command::builder("systemctl")
            .arg("is-active")
            .arg("ssh")
            .build();
        
        let result = executor.execute(&target, cmd).await.unwrap();
        assert!(result.success(), "SSH service should be active");
        
        // List all services
        let list_cmd = Command::builder("systemctl")
            .arg("list-units")
            .arg("--type=service")
            .arg("--no-pager")
            .arg("--no-legend")
            .build();
        
        let (mut events, mut handle) = executor.launch(&target, list_cmd).await.unwrap();
        
        let mut service_count = 0;
        while let Some(event) = events.next().await {
            if let Some(data) = &event.data {
                if data.contains(".service") {
                    service_count += 1;
                }
            }
        }
        
        let _ = handle.wait().await;
        assert!(service_count > 0, "Should see some services running");
    });
}

#[test]
fn test_sudo_systemctl_via_ssh() {
    futures::executor::block_on(async {
        ensure_container_running().await
            .expect("Failed to ensure container is running");
        
        let local = LocalLauncher;
        let ssh_launcher = SshLauncher::new(local, get_ssh_config());
        let executor = Executor::new("test-sudo-ssh".to_string(), ssh_launcher);
        
        // Test with sudo (should work without password)
        let cmd = Command::builder("sudo")
            .arg("systemctl")
            .arg("status")
            .arg("--no-pager")
            .build();
        
        let result = executor.execute(&Target::Command, cmd).await;
        assert!(result.is_ok(), "Sudo systemctl should work: {:?}", result);
    });
}