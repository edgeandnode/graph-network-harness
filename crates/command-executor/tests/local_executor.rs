//! Tests for local command execution

use command_executor::{Executor, ProcessHandle, ProcessEvent, ProcessEventType};
use command_executor::backends::local::{Command, ManagedProcess, LocalTarget};
use async_process::Command as AsyncCommand;
use futures::StreamExt;
use std::time::Duration;

#[test]
fn test_basic_echo() {
    futures::executor::block_on(async {
        let executor = Executor::local("test-echo");
        let target = LocalTarget::Command(Command::new());
        
        let mut cmd = AsyncCommand::new("echo");
        cmd.arg("hello world");
        
        let exit_status = executor.execute(&target, cmd).await.unwrap();
        
        assert_eq!(exit_status.code, Some(0));
        #[cfg(unix)]
        assert_eq!(exit_status.signal, None);
    });
}

#[test]
fn test_command_with_env_vars() {
    futures::executor::block_on(async {
        let executor = Executor::local("test-env");
        let target = LocalTarget::Command(Command::new());
        
        let mut cmd = AsyncCommand::new("sh");
        cmd.arg("-c").arg("echo $TEST_VAR");
        cmd.env("TEST_VAR", "test_value");
        
        let exit_status = executor.execute(&target, cmd).await.unwrap();
        
        assert_eq!(exit_status.code, Some(0));
    });
}

#[test]
fn test_working_directory() {
    futures::executor::block_on(async {
        let executor = Executor::local("test-pwd");
        let target = LocalTarget::Command(Command::new());
        
        let mut cmd = AsyncCommand::new("pwd");
        cmd.current_dir("/tmp");
        
        let exit_status = executor.execute(&target, cmd).await.unwrap();
        
        assert_eq!(exit_status.code, Some(0));
    });
}

#[test]
fn test_command_not_found() {
    futures::executor::block_on(async {
        let executor = Executor::local("test-not-found");
        let target = LocalTarget::Command(Command::new());
        
        let cmd = AsyncCommand::new("this_command_does_not_exist_12345");
        
        let result = executor.execute(&target, cmd).await;
        assert!(result.is_err());
    });
}

#[test]
fn test_exit_code_propagation() {
    futures::executor::block_on(async {
        let executor = Executor::local("test-exit-code");
        let target = LocalTarget::Command(Command::new());
        
        let mut cmd = AsyncCommand::new("sh");
        cmd.arg("-c").arg("exit 42");
        
        let exit_status = executor.execute(&target, cmd).await.unwrap();
        
        assert_eq!(exit_status.code, Some(42));
    });
}

#[test]
fn test_spawn_and_wait() {
    futures::executor::block_on(async {
        let executor = Executor::local("test-spawn");
        let target = LocalTarget::Command(Command::new());
        
        let mut cmd = AsyncCommand::new("sleep");
        cmd.arg("0.1");
        
        let (_events, mut handle) = executor.launch(&target, cmd).await.unwrap();
        
        // Process should have a PID
        assert!(handle.pid().is_some());
        
        // Wait for completion
        let exit_status = handle.wait().await.unwrap();
        assert_eq!(exit_status.code, Some(0));
    });
}

#[test]
#[cfg(unix)]
fn test_signal_handling() {
    futures::executor::block_on(async {
        let executor = Executor::local("test-signal");
        let target = LocalTarget::Command(Command::new());
        
        let mut cmd = AsyncCommand::new("sleep");
        cmd.arg("10");
        
        let (_events, mut handle) = executor.launch(&target, cmd).await.unwrap();
        
        // Give it a moment to start
        smol::Timer::after(Duration::from_millis(100)).await;
        
        // Send SIGTERM
        handle.terminate().await.unwrap();
        
        // Wait for exit
        let exit_status = handle.wait().await.unwrap();
        
        // Should have been terminated by signal
        assert!(exit_status.signal.is_some());
    });
}

#[test]
fn test_managed_process_target() {
    futures::executor::block_on(async {
        let executor = Executor::local("test-managed");
        let target = LocalTarget::ManagedProcess(ManagedProcess {
            process_group: None,
            restart_on_failure: false,
        });
        
        let mut cmd = AsyncCommand::new("echo");
        cmd.arg("managed process");
        
        let exit_status = executor.execute(&target, cmd).await.unwrap();
        
        assert_eq!(exit_status.code, Some(0));
    });
}

// Test for event streaming
#[test]
fn test_event_streaming() {
    futures::executor::block_on(async {
        let executor = Executor::local("test-events");
        let target = LocalTarget::Command(Command::new());
        
        let mut cmd = AsyncCommand::new("sh");
        cmd.arg("-c").arg("echo stdout; echo stderr >&2");
        
        let (mut events, mut handle) = executor.launch(&target, cmd).await.unwrap();
        
        // Collect all events from the stream
        let mut collected: Vec<ProcessEvent> = Vec::new();
        while let Some(event) = events.next().await {
            collected.push(event);
        }
        
        // Should have Started event first
        assert!(matches!(collected.first().unwrap().event_type, ProcessEventType::Started { .. }));
        
        // Should have stdout and stderr events
        assert!(collected.iter().any(|e| matches!(e.event_type, ProcessEventType::Stdout)));
        assert!(collected.iter().any(|e| matches!(e.event_type, ProcessEventType::Stderr)));
        
        // Verify the actual content of the events
        let stdout_events: Vec<_> = collected.iter()
            .filter(|e| matches!(e.event_type, ProcessEventType::Stdout))
            .collect();
        assert_eq!(stdout_events.len(), 1);
        assert_eq!(stdout_events[0].data.as_ref().unwrap(), "stdout");
        
        let stderr_events: Vec<_> = collected.iter()
            .filter(|e| matches!(e.event_type, ProcessEventType::Stderr))
            .collect();
        assert_eq!(stderr_events.len(), 1);
        assert_eq!(stderr_events[0].data.as_ref().unwrap(), "stderr");
        
        // Wait for process to complete
        let exit_status = handle.wait().await.unwrap();
        assert_eq!(exit_status.code, Some(0));
    });
}