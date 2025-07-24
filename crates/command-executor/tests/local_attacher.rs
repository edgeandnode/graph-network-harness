//! Tests for local service attachment

use command_executor::attacher::{AttachConfig, AttachedHandle, Attacher, ServiceStatus};
use command_executor::backends::local::LocalAttacher;
use command_executor::ManagedService;
use command_executor::{Command, ProcessEventType};
use futures::StreamExt;
use std::time::Duration;

#[test]
fn test_attach_to_running_service() {
    futures::executor::block_on(async {
        // This test requires a mock service that's always "running"
        let attacher = LocalAttacher;
        let service = ManagedService::builder("test-service")
            .status_command(Command::new("true")) // Always returns 0 (running)
            .start_command(Command::new("true"))
            .stop_command(Command::new("true"))
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
            if collected.len() >= 1 {
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
        let service = ManagedService::builder("stopped-service")
            .status_command(Command::new("false")) // Always returns 1 (not running)
            .start_command(Command::new("true"))
            .stop_command(Command::new("true"))
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
        let service = ManagedService::builder("lifecycle-test")
            .status_command(Command::new("true"))
            .start_command(Command::builder("echo").arg("starting service").build())
            .stop_command(Command::builder("echo").arg("stopping service").build())
            .restart_command(Command::builder("echo").arg("restarting service").build())
            .reload_command(Command::builder("echo").arg("reloading service").build())
            .log_command(Command::builder("echo").arg("test").build())
            .build()
            .unwrap();

        let config = AttachConfig::default();
        let (_events, mut handle) = attacher.attach(&service, config).await.unwrap();

        // Test status
        let status = handle.status().await.unwrap();
        assert_eq!(status, ServiceStatus::Running);

        // Test start
        assert!(handle.start().await.is_ok(), "Start should succeed");

        // Test stop
        assert!(handle.stop().await.is_ok(), "Stop should succeed");

        // Test restart
        assert!(handle.restart().await.is_ok(), "Restart should succeed");

        // Test reload
        assert!(handle.reload().await.is_ok(), "Reload should succeed");

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
        let service = ManagedService::builder("no-reload-service")
            .status_command(Command::new("true"))
            .start_command(Command::new("true"))
            .stop_command(Command::new("true"))
            .log_command(Command::builder("echo").arg("test").build())
            .build()
            .unwrap();

        let config = AttachConfig::default();
        let (_events, mut handle) = attacher.attach(&service, config).await.unwrap();

        // Reload should fail
        assert!(
            handle.reload().await.is_err(),
            "Reload should fail when not supported"
        );
    });
}

#[test]
fn test_restart_fallback() {
    futures::executor::block_on(async {
        let attacher = LocalAttacher;
        let service = ManagedService::builder("restart-fallback")
            .status_command(Command::new("true"))
            .start_command(Command::builder("echo").arg("start").build())
            .stop_command(Command::builder("echo").arg("stop").build())
            .log_command(Command::builder("echo").arg("test").build())
            .build()
            .unwrap();

        let config = AttachConfig::default();
        let (_events, mut handle) = attacher.attach(&service, config).await.unwrap();

        // Restart should succeed using stop+start fallback
        assert!(
            handle.restart().await.is_ok(),
            "Restart should succeed with fallback"
        );
    });
}

#[test]
fn test_attach_config_options() {
    futures::executor::block_on(async {
        let attacher = LocalAttacher;

        // Test with tail -n for history lines
        let service = ManagedService::builder("config-test")
            .status_command(Command::new("true"))
            .start_command(Command::new("true"))
            .stop_command(Command::new("true"))
            .log_command(Command::new("tail")) // Use tail to test -n flag
            .build()
            .unwrap();

        let mut config = AttachConfig::default();
        config.history_lines = Some(50);
        config.follow_from_start = true;

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
        let service = ManagedService::builder("failure-test")
            .status_command(Command::new("true"))
            .start_command(Command::builder("sh").arg("-c").arg("exit 1").build())
            .stop_command(Command::builder("sh").arg("-c").arg("exit 1").build())
            .log_command(Command::builder("echo").arg("test").build())
            .build()
            .unwrap();

        let config = AttachConfig::default();
        let (_events, mut handle) = attacher.attach(&service, config).await.unwrap();

        // Start should fail
        assert!(
            handle.start().await.is_err(),
            "Start should fail with exit code 1"
        );

        // Stop should fail
        assert!(
            handle.stop().await.is_err(),
            "Stop should fail with exit code 1"
        );
    });
}

#[test]
fn test_systemd_status_codes() {
    futures::executor::block_on(async {
        // Test the status code interpretation logic
        let attacher = LocalAttacher;

        // Test exit code 3 (stopped in systemd)
        let service = ManagedService::builder("systemd-stopped")
            .status_command(Command::builder("sh").arg("-c").arg("exit 3").build())
            .start_command(Command::new("true"))
            .stop_command(Command::new("true"))
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
        let service = ManagedService::builder("cleanup-test")
            .status_command(Command::new("true"))
            .start_command(Command::new("true"))
            .stop_command(Command::new("true"))
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
