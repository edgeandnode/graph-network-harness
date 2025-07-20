//! Integration tests for systemd-portable that run in a real systemd container
//! 
//! These tests require the systemd container to be running.
//! Run them with: ./tests/systemd-container/run-systemd-tests.sh

#![cfg(all(test, unix))]

use command_executor::{Executor, ProcessEventType, ProcessHandle, Command};
use command_executor::backends::local::{SystemdPortable, LocalTarget};
use futures::StreamExt;
use std::time::Duration;

fn is_in_systemd_container() -> bool {
    // Check if we're running inside the systemd container
    std::path::Path::new("/opt/portable-services").exists() &&
    std::process::Command::new("systemctl")
        .arg("is-system-running")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
#[ignore = "Requires SSH access to systemd-enabled Docker container"]
fn test_real_portablectl_attach_detach() {
    if !is_in_systemd_container() {
        eprintln!("Skipping test - not in systemd container");
        return;
    }
    
    futures::executor::block_on(async {
        let executor = Executor::local("systemd-integration");
        let target = LocalTarget::SystemdPortable(SystemdPortable::new(
            "/opt/portable-services/echo-service.tar.gz",
            "echo-service.service"
        ));
        
        // First, ensure the service is not attached
        let detach_cmd = Command::builder("portablectl")
            .arg("detach")
            .arg("echo-service.tar.gz")
            .build()
            .prepare();
        let _ = executor.launch(&target, detach_cmd).await; // Ignore errors if not attached
        
        // Attach the portable service
        let attach_cmd = Command::builder("portablectl")
            .arg("attach")
            .arg("--copy=copy")  // Use copy mode for testing
            .arg("/opt/portable-services/echo-service.tar.gz")
            .build()
            .prepare();
        
        let result = executor.launch(&target, attach_cmd).await;
        assert!(result.is_ok(), "Failed to attach portable service");
        
        let (_events, mut handle) = result.unwrap();
        let exit_status = handle.wait().await.unwrap();
        assert_eq!(exit_status.code, Some(0), "portablectl attach should succeed");
        
        // Verify the service is attached
        let list_cmd = Command::builder("portablectl")
            .arg("list")
            .build()
            .prepare();
        
        let (mut events, mut handle) = executor.launch(&target, list_cmd).await.unwrap();
        
        let mut output = String::new();
        while let Some(event) = events.next().await {
            if let ProcessEventType::Stdout = event.event_type {
                if let Some(data) = event.data {
                    output.push_str(&data);
                    output.push('\n');
                }
            }
        }
        
        handle.wait().await.unwrap();
        assert!(output.contains("echo-service"), "Service should be listed after attach");
        
        // Clean up: detach the service
        let detach_cmd = Command::builder("portablectl")
            .arg("detach")
            .arg("echo-service.tar.gz")
            .build()
            .prepare();
        
        let (_events, mut handle) = executor.launch(&target, detach_cmd).await.unwrap();
        let exit_status = handle.wait().await.unwrap();
        assert_eq!(exit_status.code, Some(0), "portablectl detach should succeed");
    });
}

#[test]
#[ignore = "Requires SSH access to systemd-enabled Docker container"]
fn test_portable_service_lifecycle() {
    if !is_in_systemd_container() {
        eprintln!("Skipping test - not in systemd container");
        return;
    }
    
    futures::executor::block_on(async {
        let executor = Executor::local("systemd-lifecycle");
        let target = LocalTarget::SystemdPortable(SystemdPortable::new(
            "/opt/portable-services/counter-service.tar.gz",
            "counter-service.service"
        ));
        
        // Clean slate
        let detach_cmd = Command::builder("portablectl")
            .arg("detach")
            .arg("counter-service.tar.gz")
            .build()
            .prepare();
        let _ = executor.launch(&target, detach_cmd).await;
        
        // Attach the service
        let attach_cmd = Command::builder("portablectl")
            .arg("attach")
            .arg("--copy=copy")
            .arg("/opt/portable-services/counter-service.tar.gz")
            .build()
            .prepare();
        
        let (_events, mut handle) = executor.launch(&target, attach_cmd).await.unwrap();
        handle.wait().await.unwrap();
        
        // Start the service
        let start_cmd = Command::builder("systemctl")
            .arg("start")
            .arg("counter-service.service")
            .build()
            .prepare();
        
        let (_events, mut handle) = executor.launch(&target, start_cmd).await.unwrap();
        handle.wait().await.unwrap();
        
        // Give it time to run
        smol::Timer::after(Duration::from_secs(3)).await;
        
        // Check service status
        let status_cmd = Command::builder("systemctl")
            .arg("is-active")
            .arg("counter-service.service")
            .build()
            .prepare();
        
        let (mut events, mut handle) = executor.launch(&target, status_cmd).await.unwrap();
        
        let mut status_output = String::new();
        while let Some(event) = events.next().await {
            if let ProcessEventType::Stdout = event.event_type {
                if let Some(data) = event.data {
                    status_output.push_str(&data);
                }
            }
        }
        
        let exit_status = handle.wait().await.unwrap();
        assert_eq!(exit_status.code, Some(0), "Service should be active");
        assert!(status_output.contains("active"), "Service status should be active");
        
        // Stop the service
        let stop_cmd = Command::builder("systemctl")
            .arg("stop")
            .arg("counter-service.service")
            .build()
            .prepare();
        
        let (_events, mut handle) = executor.launch(&target, stop_cmd).await.unwrap();
        handle.wait().await.unwrap();
        
        // Detach the service
        let detach_cmd = Command::builder("portablectl")
            .arg("detach")
            .arg("counter-service.tar.gz")
            .build()
            .prepare();
        
        let (_events, mut handle) = executor.launch(&target, detach_cmd).await.unwrap();
        handle.wait().await.unwrap();
    });
}

#[test]
#[ignore = "Requires SSH access to systemd-enabled Docker container"]
fn test_portable_service_logs() {
    if !is_in_systemd_container() {
        eprintln!("Skipping test - not in systemd container");
        return;
    }
    
    futures::executor::block_on(async {
        let executor = Executor::local("systemd-logs");
        let target = LocalTarget::SystemdPortable(SystemdPortable::new(
            "/opt/portable-services/echo-service.tar.gz",
            "echo-service.service"
        ));
        
        // Clean slate
        let detach_cmd = Command::builder("portablectl")
            .arg("detach")
            .arg("echo-service.tar.gz")
            .build()
            .prepare();
        let _ = executor.launch(&target, detach_cmd).await;
        
        // Attach and start the service
        let attach_cmd = Command::builder("portablectl")
            .arg("attach")
            .arg("--copy=copy")
            .arg("--now")  // Start immediately
            .arg("/opt/portable-services/echo-service.tar.gz")
            .build()
            .prepare();
        
        let (_events, mut handle) = executor.launch(&target, attach_cmd).await.unwrap();
        handle.wait().await.unwrap();
        
        // Give it time to generate some logs
        smol::Timer::after(Duration::from_secs(2)).await;
        
        // Read logs with journalctl
        let logs_cmd = Command::builder("journalctl")
            .arg("-u")
            .arg("echo-service.service")
            .arg("--no-pager")
            .arg("-n")
            .arg("10")
            .build()
            .prepare();
        
        let (mut events, mut handle) = executor.launch(&target, logs_cmd).await.unwrap();
        
        let mut log_output = String::new();
        while let Some(event) = events.next().await {
            if let ProcessEventType::Stdout = event.event_type {
                if let Some(data) = event.data {
                    log_output.push_str(&data);
                    log_output.push('\n');
                }
            }
        }
        
        handle.wait().await.unwrap();
        
        // Verify logs contain expected output
        assert!(log_output.contains("Echo service"), "Logs should contain service output");
        
        // Stop and detach
        let stop_cmd = Command::builder("systemctl")
            .arg("stop")
            .arg("echo-service.service")
            .build()
            .prepare();
        let (_events, mut handle) = executor.launch(&target, stop_cmd).await.unwrap();
        handle.wait().await.unwrap();
        
        let detach_cmd = Command::builder("portablectl")
            .arg("detach")
            .arg("echo-service.tar.gz")
            .build()
            .prepare();
        let (_events, mut handle) = executor.launch(&target, detach_cmd).await.unwrap();
        handle.wait().await.unwrap();
    });
}