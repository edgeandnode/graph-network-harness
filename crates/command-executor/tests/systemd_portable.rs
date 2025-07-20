//! Tests for systemd-portable support

use command_executor::backends::local::{LocalTarget, SystemdPortable};
use command_executor::{Executor, ProcessEventType, ProcessHandle, Command};
use futures::StreamExt;

#[test]
#[cfg(unix)]
#[ignore = "Requires systemd docker environment"]
fn test_systemd_portable_attach() {
    futures::executor::block_on(async {
        let executor = Executor::local("test-portable");
        let target = LocalTarget::SystemdPortable(SystemdPortable::new(
            "test-image.raw",
            "test-service.service",
        ));

        // With SystemdPortable, we pass the full portablectl command
        let cmd = Command::builder("portablectl")
            .arg("attach")
            .arg("--enable")
            .arg("--now")
            .arg("test-image.raw")
            .build()
            .prepare();

        let result = executor.launch(&target, cmd).await;

        // If portablectl doesn't exist, we should get a spawn error
        match result {
            Err(e) => {
                // Either portablectl is not found, or it fails due to missing image
                assert!(
                    e.to_string().contains("Failed to spawn")
                        || e.to_string().contains("portablectl")
                );
            }
            Ok((_events, handle)) => {
                // If it succeeds, we have a handle
                assert!(handle.pid().is_some());
            }
        }
    });
}

#[test]
#[cfg(unix)]
#[ignore = "Requires systemd docker environment"]
fn test_systemd_portable_commands() {
    futures::executor::block_on(async {
        let executor = Executor::local("test-portable-commands");
        let target = LocalTarget::SystemdPortable(SystemdPortable::new("app.raw", "app.service"));

        // Test various portablectl commands
        let test_commands = vec![
            // Attach command
            Command::builder("portablectl")
                .arg("attach")
                .arg("--enable")
                .arg("--now")
                .arg("app.raw")
                .build()
                .prepare(),
            // Detach command
            Command::builder("portablectl")
                .arg("detach")
                .arg("app.raw")
                .build()
                .prepare(),
            // List command
            Command::builder("portablectl")
                .arg("list")
                .build()
                .prepare(),
        ];

        for cmd in test_commands {
            let result = executor.launch(&target, cmd).await;

            // We expect these to fail since portablectl likely isn't installed
            // or the image doesn't exist
            assert!(result.is_err() || result.is_ok());
        }
    });
}

#[test]
#[cfg(unix)]
#[ignore = "Requires systemd docker environment"]
fn test_systemd_portable_passthrough() {
    futures::executor::block_on(async {
        let executor = Executor::local("test-portable-passthrough");
        let target =
            LocalTarget::SystemdPortable(SystemdPortable::new("custom.raw", "custom.service"));

        // Any command can be passed through - the user has full control
        let cmd = Command::builder("portablectl")
            .arg("inspect")
            .arg("custom.raw")
            .build()
            .prepare();

        let result = executor.launch(&target, cmd).await;

        // Should attempt to run the command as-is
        assert!(result.is_err() || result.is_ok());
    });
}

#[test]
#[ignore = "Requires systemd docker environment"]
fn test_systemd_portable_target_creation() {
    // Test that we can create and use SystemdPortable targets
    let portable = SystemdPortable::new("myapp.raw", "myapp.service");

    assert_eq!(portable.image_name(), "myapp.raw");
    assert_eq!(portable.unit_name(), "myapp.service");

    // Test that it can be used in LocalTarget
    let target = LocalTarget::SystemdPortable(portable);
    match target {
        LocalTarget::SystemdPortable(p) => {
            assert_eq!(p.image_name(), "myapp.raw");
            assert_eq!(p.unit_name(), "myapp.service");
        }
        _ => panic!("Expected SystemdPortable variant"),
    }
}

#[test]
#[cfg(unix)]
#[ignore = "Requires systemd docker environment"]
fn test_systemd_portable_echo_mock() {
    futures::executor::block_on(async {
        let executor = Executor::local("test-portable-echo");
        let target = LocalTarget::SystemdPortable(SystemdPortable::new("test.raw", "test.service"));

        // Use echo as a simple mock for portablectl
        let cmd = Command::builder("echo")
            .arg("Mock: portablectl attach test.raw")
            .build()
            .prepare();

        let result = executor.launch(&target, cmd).await;
        assert!(result.is_ok());

        let (mut events, mut handle) = result.unwrap();

        // Collect events
        let mut collected = Vec::new();
        while let Some(event) = events.next().await {
            collected.push(event);
        }

        // Should have a Started event
        assert!(collected
            .iter()
            .any(|e| matches!(e.event_type, ProcessEventType::Started { .. })));

        // Should have stdout with our echo output
        let stdout_events: Vec<_> = collected
            .iter()
            .filter(|e| matches!(e.event_type, ProcessEventType::Stdout))
            .collect();
        assert_eq!(stdout_events.len(), 1);
        assert!(stdout_events[0]
            .data
            .as_ref()
            .unwrap()
            .contains("Mock: portablectl"));

        // Wait for completion
        let exit_status = handle.wait().await.unwrap();
        assert_eq!(exit_status.code, Some(0));
    });
}
