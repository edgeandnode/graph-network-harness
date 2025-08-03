//! Integration tests for local attacher

use command_executor::{
    backends::LocalAttacher,
    attacher::{Attacher, AttachConfig, AttachedHandle, ServiceStatus},
    target::AttachedService,
    Command,
};
use futures::StreamExt;

#[smol_potat::test]
async fn test_local_attacher_not_running_service() {
    let attacher = LocalAttacher;
    
    // Create a service that checks for a non-existent process
    let service = AttachedService::builder("fake-service")
        .status_command(Command::new("false")) // Always returns non-zero
        .log_command(Command::new("tail").arg("-f").arg("/dev/null").clone())
        .build()
        .unwrap();
    
    let config = AttachConfig::default();
    let result = attacher.attach(&service, config).await;
    
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.to_string().contains("not running"));
    } else {
        panic!("Expected error for non-running service");
    }
}

#[smol_potat::test]
async fn test_local_attacher_with_running_service() {
    let attacher = LocalAttacher;
    
    // Create a service that simulates a running service
    let service = AttachedService::builder("test-service")
        .status_command(Command::new("true")) // Always returns 0 (running)
        .log_command(Command::new("echo").arg("test log output").clone())
        .build()
        .unwrap();
    
    let config = AttachConfig {
        follow_from_start: false,
        history_lines: None,
        timeout_seconds: Some(5),
    };
    
    let result = attacher.attach(&service, config).await;
    assert!(result.is_ok());
    
    let (mut events, mut handle) = result.unwrap();
    
    // Check handle ID
    assert_eq!(handle.id(), "test-service");
    
    // Collect some events
    let mut output = String::new();
    let timeout = std::time::Duration::from_secs(1);
    let start = std::time::Instant::now();
    
    while start.elapsed() < timeout {
        if let Ok(Some(event)) = smol::future::or(
            async { Ok(events.next().await) },
            async {
                smol::Timer::after(std::time::Duration::from_millis(100)).await;
                Err(())
            }
        ).await {
            if let Some(data) = &event.data {
                output.push_str(data);
            }
        } else {
            break;
        }
    }
    
    assert!(output.contains("test log output"));
    
    // Check status
    let status = handle.status().await.unwrap();
    assert_eq!(status, ServiceStatus::Running);
    
    // Disconnect
    assert!(handle.disconnect().await.is_ok());
}

#[smol_potat::test]
async fn test_local_attacher_with_tail_flags() {
    let attacher = LocalAttacher;
    
    // Create a test file to tail
    let temp_file = "/tmp/test_attacher_tail.txt";
    std::fs::write(temp_file, "line1\nline2\nline3\nline4\nline5\n").unwrap();
    
    let service = AttachedService::builder("tail-test")
        .status_command(Command::new("true"))
        .log_command(Command::new("tail").arg(temp_file).clone())
        .build()
        .unwrap();
    
    let config = AttachConfig {
        follow_from_start: true, // Should add -f flag
        history_lines: Some(3),  // Should add -n 3
        timeout_seconds: Some(5),
    };
    
    let result = attacher.attach(&service, config).await;
    assert!(result.is_ok());
    
    let (mut events, mut handle) = result.unwrap();
    
    // Collect output
    let mut lines = Vec::new();
    let timeout = std::time::Duration::from_millis(500);
    let start = std::time::Instant::now();
    
    while start.elapsed() < timeout {
        if let Ok(Some(event)) = smol::future::or(
            async { Ok(events.next().await) },
            async {
                smol::Timer::after(std::time::Duration::from_millis(50)).await;
                Err(())
            }
        ).await {
            if let Some(data) = &event.data {
                lines.push(data.clone());
            }
        } else {
            break;
        }
    }
    
    // We should get the last 3 lines due to history_lines setting
    assert!(lines.len() >= 3);
    assert!(lines.iter().any(|l| l.contains("line3")));
    assert!(lines.iter().any(|l| l.contains("line4")));
    assert!(lines.iter().any(|l| l.contains("line5")));
    
    // Clean up
    handle.disconnect().await.unwrap();
    std::fs::remove_file(temp_file).ok();
}

#[smol_potat::test]
async fn test_local_attacher_journalctl_flags() {
    let attacher = LocalAttacher;
    
    // Test with journalctl-like command
    let service = AttachedService::builder("journal-test")
        .status_command(Command::new("true"))
        .log_command(Command::new("journalctl")) // Would need actual args in real use
        .build()
        .unwrap();
    
    let config = AttachConfig {
        follow_from_start: true,
        history_lines: Some(50),
        timeout_seconds: Some(1),
    };
    
    // This test just verifies the attacher doesn't crash with journalctl commands
    // In a real environment, journalctl would need proper unit specification
    let result = attacher.attach(&service, config).await;
    
    // It's ok if this fails (journalctl might not be available or might need args)
    // We're just testing that the flag logic doesn't panic
    if let Ok((_events, mut handle)) = result {
        handle.disconnect().await.ok();
    }
}

#[smol_potat::test]  
async fn test_local_attacher_handle_drop() {
    let attacher = LocalAttacher;
    
    let service = AttachedService::builder("drop-test")
        .status_command(Command::new("true"))
        .log_command(Command::new("cat").arg("/dev/zero").clone()) // Infinite output
        .build()
        .unwrap();
    
    let config = AttachConfig::default();
    let result = attacher.attach(&service, config).await;
    assert!(result.is_ok());
    
    let (_events, handle) = result.unwrap();
    
    // Just drop the handle - should clean up the log process
    drop(handle);
    
    // Give it a moment to clean up
    smol::Timer::after(std::time::Duration::from_millis(100)).await;
    
    // Test passes if we get here without hanging
}

#[smol_potat::test]
async fn test_local_attacher_event_stream() {
    let attacher = LocalAttacher;
    
    // Create a service that outputs multiple lines
    let service = AttachedService::builder("stream-test")
        .status_command(Command::new("true"))
        .log_command(Command::new("sh")
            .arg("-c")
            .arg("echo 'line 1'; echo 'line 2'; echo 'line 3'")
            .clone())
        .build()
        .unwrap();
    
    let config = AttachConfig::default();
    let result = attacher.attach(&service, config).await;
    assert!(result.is_ok());
    
    let (mut events, mut handle) = result.unwrap();
    
    // Collect all events
    let mut event_count = 0;
    while let Some(event) = events.next().await {
        if event.data.is_some() {
            event_count += 1;
        }
    }
    
    assert_eq!(event_count, 3);
    
    handle.disconnect().await.unwrap();
}