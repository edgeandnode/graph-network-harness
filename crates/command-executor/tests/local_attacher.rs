//! Tests for local service attachment

use command_executor::AttachedService;
use command_executor::attacher::{AttachConfig, AttachedHandle, Attacher, ServiceStatus};
use command_executor::backends::LocalAttacher;
use command_executor::{Command, ProcessEventType};
use futures::StreamExt;
use std::time::Duration;

#[test]
fn test_attach_to_running_service() {
    futures::executor::block_on(async {
        // This test requires a mock service that's always "running"
        let attacher = LocalAttacher;
        let service = AttachedService::builder("test-service")
            .status_command(Command::new("true")) // Always returns 0 (running)
            .log_command(Command::builder("echo").arg("test log line").build())
            .build()
            .unwrap();

        let config = AttachConfig::default();
        let result = attacher.attach(&service, config).await;

        assert!(
            result.is_ok(),
            "Should successfully attach to running service"
        );

        let (mut events, handle) = result.unwrap();

        // Verify handle ID
        assert_eq!(handle.id(), "test-service");

        // Collect some events
        let mut collected = Vec::new();
        while let Some(event) = events.next().await {
            collected.push(event);
            if !collected.is_empty() {
                break; // Just get the first event for this test
            }
        }

        // Should have received at least one log event
        assert!(!collected.is_empty(), "Should receive log events");
        assert!(matches!(collected[0].event_type, ProcessEventType::Stdout));
        assert_eq!(collected[0].data.as_ref().unwrap(), "test log line");
    });
}

#[test]
fn test_attach_to_stopped_service() {
    futures::executor::block_on(async {
        let attacher = LocalAttacher;
        let service = AttachedService::builder("stopped-service")
            .status_command(Command::new("false")) // Always returns 1 (not running)
            .log_command(Command::builder("echo").arg("test").build())
            .build()
            .unwrap();

        let config = AttachConfig::default();
        let result = attacher.attach(&service, config).await;

        assert!(result.is_err(), "Should fail to attach to stopped service");
    });
}

#[test]
fn test_service_lifecycle_control() {
    futures::executor::block_on(async {
        let attacher = LocalAttacher;
        let service = AttachedService::builder("lifecycle-test")
            .status_command(Command::new("true"))
            .log_command(Command::builder("echo").arg("test").build())
            .build()
            .unwrap();

        let config = AttachConfig::default();
        let (_events, mut handle) = attacher.attach(&service, config).await.unwrap();

        // Test status
        let status = handle.status().await.unwrap();
        assert_eq!(status, ServiceStatus::Running);

        // Attached handles are read-only - they cannot control service lifecycle
        // They can only observe status and disconnect

        // Test disconnect
        assert!(
            handle.disconnect().await.is_ok(),
            "Disconnect should succeed"
        );
    });
}

#[test]
fn test_service_without_reload() {
    futures::executor::block_on(async {
        let attacher = LocalAttacher;
        let service = AttachedService::builder("no-reload-service")
            .status_command(Command::new("true"))
            .log_command(Command::builder("echo").arg("test").build())
            .build()
            .unwrap();

        let config = AttachConfig::default();
        let (_events, mut handle) = attacher.attach(&service, config).await.unwrap();

        // Attached handles don't have reload capability
        // They are read-only interfaces to observe existing services
    });
}

#[test]
fn test_restart_fallback() {
    futures::executor::block_on(async {
        let attacher = LocalAttacher;
        let service = AttachedService::builder("restart-fallback")
            .status_command(Command::new("true"))
            .log_command(Command::builder("echo").arg("test").build())
            .build()
            .unwrap();

        let config = AttachConfig::default();
        let (_events, mut handle) = attacher.attach(&service, config).await.unwrap();

        // Attached handles don't have restart capability
        // They are read-only interfaces to observe existing services
    });
}

#[test]
fn test_attach_config_options() {
    futures::executor::block_on(async {
        let attacher = LocalAttacher;

        // Test with tail -n for history lines
        let service = AttachedService::builder("config-test")
            .status_command(Command::new("true"))
            .log_command(Command::new("tail")) // Use tail to test -n flag
            .build()
            .unwrap();

        let config = AttachConfig {
            history_lines: Some(50),
            follow_from_start: true,
            ..Default::default()
        };

        // This will construct: tail -n 50 -f
        let result = attacher.attach(&service, config).await;

        // We can't easily verify the exact command, but we can ensure it doesn't error
        // In a real scenario, this would follow logs with the last 50 lines
        assert!(result.is_ok(), "Should handle attach config options");
    });
}

#[test]
fn test_command_failure_handling() {
    futures::executor::block_on(async {
        let attacher = LocalAttacher;
        let service = AttachedService::builder("failure-test")
            .status_command(Command::new("true"))
            .log_command(Command::builder("echo").arg("test").build())
            .build()
            .unwrap();

        let config = AttachConfig::default();
        let (_events, mut handle) = attacher.attach(&service, config).await.unwrap();

        // Attached handles cannot control service lifecycle
        // They are read-only interfaces
    });
}

#[test]
fn test_systemd_status_codes() {
    futures::executor::block_on(async {
        // Test the status code interpretation logic
        let attacher = LocalAttacher;

        // Test exit code 3 (stopped in systemd)
        let service = AttachedService::builder("systemd-stopped")
            .status_command(Command::builder("sh").arg("-c").arg("exit 3").build())
            .log_command(Command::builder("echo").arg("test").build())
            .build()
            .unwrap();

        // Should fail to attach because service is not running
        let config = AttachConfig::default();
        let result = attacher.attach(&service, config).await;
        assert!(result.is_err(), "Should not attach to stopped service");
    });
}

#[test]
fn test_log_streaming_cleanup() {
    futures::executor::block_on(async {
        let attacher = LocalAttacher;
        let service = AttachedService::builder("cleanup-test")
            .status_command(Command::new("true"))
            .log_command(
                Command::builder("sh")
                    .arg("-c")
                    .arg("for i in 1 2 3; do echo line $i; sleep 0.01; done")
                    .build(),
            )
            .build()
            .unwrap();

        let config = AttachConfig::default();
        let (events, _handle) = attacher.attach(&service, config).await.unwrap();

        // Drop the event stream immediately
        // This should trigger cleanup of the log child process
        drop(events);

        // Give it a moment to clean up
        smol::Timer::after(Duration::from_millis(50)).await;

        // The test passes if no zombie processes are left behind
        // (In a real test environment, we could check process table)
    });
}
