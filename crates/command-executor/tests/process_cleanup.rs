//! Tests for process cleanup on drop

use command_executor::command::Command;
use command_executor::Target;
use command_executor::{Executor, ProcessHandle};
use std::time::Duration;

#[test]
#[cfg(unix)]
fn test_process_cleanup_on_handle_drop() {
    futures::executor::block_on(async {
        let executor = Executor::local("test-cleanup");
        let target = Target::Command;

        // Start a long-running process
        let mut cmd = Command::new("sleep");
        cmd.arg("60"); // Sleep for 60 seconds

        let (_events, handle) = executor.launch(&target, cmd).await.unwrap();

        // Get the PID before dropping
        let pid = handle.pid().unwrap();

        // Drop the handle - this should kill the process
        drop(handle);

        // Give it a moment to clean up
        smol::Timer::after(Duration::from_millis(100)).await;

        // Check if process is still running
        // On Unix, we can check by sending signal 0
        use nix::sys::signal;
        use nix::unistd::Pid;

        let nix_pid = Pid::from_raw(pid as i32);
        let is_alive = signal::kill(nix_pid, None).is_ok();

        assert!(!is_alive, "Process should be killed when handle is dropped");
    });
}

#[test]
#[cfg(unix)]
fn test_process_cleanup_on_stream_drop() {
    futures::executor::block_on(async {
        let executor = Executor::local("test-stream-cleanup");
        let target = Target::Command;

        // Start a process that outputs continuously
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg("while true; do echo test; sleep 0.1; done");

        let (events, mut handle) = executor.launch(&target, cmd).await.unwrap();

        // Get the PID
        let pid = handle.pid().unwrap();

        // Drop the event stream - process should continue running
        drop(events);

        // Give it a moment
        smol::Timer::after(Duration::from_millis(100)).await;

        // Check if process is still running
        use nix::sys::signal;
        use nix::unistd::Pid;

        let nix_pid = Pid::from_raw(pid as i32);
        let is_alive = signal::kill(nix_pid, None).is_ok();

        assert!(
            is_alive,
            "Process should still be running after dropping event stream"
        );

        // Now kill it manually
        handle.kill().await.unwrap();
    });
}

#[test]
fn test_cleanup_already_exited_process() {
    futures::executor::block_on(async {
        let executor = Executor::local("test-exited-cleanup");
        let target = Target::Command;

        // Start a process that exits quickly
        let mut cmd = Command::new("echo");
        cmd.arg("quick exit");

        let (_events, mut handle) = executor.launch(&target, cmd).await.unwrap();

        // Wait for it to exit
        let exit_status = handle.wait().await.unwrap();
        assert_eq!(exit_status.code, Some(0));

        // Drop should not panic for already-exited process
        drop(handle);

        // Test passes if no panic occurs
    });
}

#[test]
#[cfg(unix)]
fn test_cleanup_with_multiple_processes() {
    futures::executor::block_on(async {
        let executor = Executor::local("test-multi-cleanup");
        let target = Target::Command;

        // Start multiple processes
        let mut handles = Vec::new();
        let mut pids = Vec::new();

        for i in 0..3 {
            let mut cmd = Command::new("sleep");
            cmd.arg(format!("{}", 60 + i)); // Different sleep times

            let (_events, handle) = executor.launch(&target, cmd).await.unwrap();
            pids.push(handle.pid().unwrap());
            handles.push(handle);
        }

        // Drop all handles
        drop(handles);

        // Give them a moment to clean up
        smol::Timer::after(Duration::from_millis(200)).await;

        // Check that all processes are killed
        use nix::sys::signal;
        use nix::unistd::Pid;

        for pid in pids {
            let nix_pid = Pid::from_raw(pid as i32);
            let is_alive = signal::kill(nix_pid, None).is_ok();
            assert!(!is_alive, "Process {} should be killed", pid);
        }
    });
}
