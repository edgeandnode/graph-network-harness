//! Integration tests for local launcher

use command_executor::{
    backends::LocalLauncher,
    launcher::Launcher,
    target::{Target, ManagedProcess},
    process::ProcessHandle,
    Command,
};
use futures::StreamExt;

#[smol_potat::test]
async fn test_local_launcher_basic_command() {
    let launcher = LocalLauncher;
    let mut command = Command::new("echo");
    command.arg("hello world");
    
    let result = launcher.launch(&Target::Command, command).await;
    assert!(result.is_ok(), "Failed to launch command: {:?}", result.err());
    
    let (mut events, mut handle) = result.unwrap();
    
    // Collect some events
    let mut output = String::new();
    while let Some(event) = events.next().await {
        if let Some(data) = &event.data {
            output.push_str(data);
            output.push('\n');
        }
    }
    
    // Wait for process to complete
    let exit_status = handle.wait().await.unwrap();
    assert_eq!(exit_status.code, Some(0));
    assert!(output.contains("hello world"));
}

// TODO: Re-enable this test once stdin forwarding task is implemented
#[ignore = "Stdin forwarding task not yet implemented"]
#[smol_potat::test]
async fn test_local_launcher_with_stdin() {
    let launcher = LocalLauncher;
    let mut command = Command::new("cat");
    
    // Set up stdin channel
    let (tx, rx) = async_channel::bounded(10);
    command.stdin_channel(rx);
    
    let result = launcher.launch(&Target::Command, command).await;
    assert!(result.is_ok());
    
    let (mut events, mut handle) = result.unwrap();
    
    // Send data through stdin
    tx.send("test input".to_string()).await.unwrap();
    tx.send("second line".to_string()).await.unwrap();
    drop(tx); // Close channel to signal EOF
    
    // Collect output
    let mut output = String::new();
    while let Some(event) = events.next().await {
        if let Some(data) = &event.data {
            output.push_str(data);
            output.push('\n');
        }
    }
    
    let exit_status = handle.wait().await.unwrap();
    assert_eq!(exit_status.code, Some(0));
    assert!(output.contains("test input"));
    assert!(output.contains("second line"));
}

#[smol_potat::test]
async fn test_local_launcher_process_handle() {
    let launcher = LocalLauncher;
    let mut command = Command::new("sleep");
    command.arg("10"); // Long-running process
    
    let result = launcher.launch(&Target::Command, command).await;
    assert!(result.is_ok());
    
    let (_events, mut handle) = result.unwrap();
    
    // Check PID
    assert!(handle.pid().is_some());
    let pid = handle.pid().unwrap();
    assert!(pid > 0);
    
    // Test termination
    let terminate_result = handle.terminate().await;
    assert!(terminate_result.is_ok());
    
    // Wait for exit
    let exit_status = handle.wait().await.unwrap();
    
    // On Unix, SIGTERM should result in signal 15
    #[cfg(unix)]
    assert_eq!(exit_status.signal, Some(15));
}

#[smol_potat::test]
async fn test_local_launcher_managed_process() {
    let launcher = LocalLauncher;
    
    let process = ManagedProcess::new();
    
    let target = Target::ManagedProcess(process);
    let mut command = Command::new("echo");
    command.arg("managed process");
    
    let result = launcher.launch(&target, command).await;
    assert!(result.is_ok());
    
    let (mut events, mut handle) = result.unwrap();
    
    // Collect output
    let mut output = String::new();
    while let Some(event) = events.next().await {
        if let Some(data) = &event.data {
            output.push_str(data);
        }
    }
    
    let exit_status = handle.wait().await.unwrap();
    assert_eq!(exit_status.code, Some(0));
    assert!(output.contains("managed process"));
}

#[smol_potat::test]
async fn test_local_launcher_drop_kills_process() {
    let launcher = LocalLauncher;
    let mut command = Command::new("sleep");
    command.arg("60"); // Long-running process
    
    let result = launcher.launch(&Target::Command, command).await;
    assert!(result.is_ok());
    
    let (_events, handle) = result.unwrap();
    let pid = handle.pid().unwrap();
    
    // Drop the handle - this should kill the process
    drop(handle);
    
    // Give it a moment to clean up
    smol::Timer::after(std::time::Duration::from_millis(100)).await;
    
    // Check if process is still running (it shouldn't be)
    #[cfg(unix)]
    {
        use nix::sys::signal;
        use nix::unistd::Pid;
        
        // Signal 0 checks if process exists without killing it
        let result = signal::kill(Pid::from_raw(pid as i32), None);
        assert!(result.is_err()); // Process should not exist
    }
}

#[smol_potat::test]
async fn test_local_launcher_execute_method() {
    let launcher = LocalLauncher;
    let mut command = Command::new("echo");
    command.arg("execute test");
    
    let result = launcher.execute(&Target::Command, command).await;
    assert!(result.is_ok());
    
    let exit_result = result.unwrap();
    assert_eq!(exit_result.status.code, Some(0));
    assert!(exit_result.output.contains("execute test"));
}