//! Tests for stdin forwarding through layered execution

use command_executor::{
    Command, ProcessHandle, Target,
    backends::LocalLauncher,
    layered::{LayeredExecutor, LocalLayer},
};
use futures::StreamExt;

#[smol_potat::test]
async fn test_stdin_forwarding_with_local_layer() {
    let executor = LayeredExecutor::new(LocalLauncher).with_layer(LocalLayer::new());

    let mut command = Command::new("cat");

    // Set up stdin channel
    let (tx, rx) = async_channel::bounded(10);
    command.stdin_channel(rx);

    let result = executor.execute(command, &Target::Command).await;
    assert!(result.is_ok());

    let (mut events, mut handle) = result.unwrap();

    // Start stdin forwarding task
    if let Some(stdin_handle) = handle.take_stdin_for_forwarding() {
        smol::spawn(async move {
            let _ = stdin_handle.forward_channel().await;
        })
        .detach();
    }

    // Send data through stdin
    tx.send("layered test input".to_string()).await.unwrap();
    tx.send("second layered line".to_string()).await.unwrap();
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
    assert!(output.contains("layered test input"));
    assert!(output.contains("second layered line"));
}

// Test stdin forwarding with multiple layers (simulated environment)
#[smol_potat::test]
async fn test_stdin_forwarding_with_env_layers() {
    let executor = LayeredExecutor::new(LocalLauncher)
        .with_layer(LocalLayer::new().with_env("TEST_VAR", "test_value"))
        .with_env("GLOBAL_VAR", "global_value");

    // Use a command that echoes stdin and environment
    let mut command = Command::new("sh");
    command.arg("-c").arg("cat && echo \"TEST_VAR=$TEST_VAR\"");

    // Set up stdin channel
    let (tx, rx) = async_channel::bounded(10);
    command.stdin_channel(rx);

    let result = executor.execute(command, &Target::Command).await;
    assert!(result.is_ok());

    let (mut events, mut handle) = result.unwrap();

    // Start stdin forwarding task
    if let Some(stdin_handle) = handle.take_stdin_for_forwarding() {
        smol::spawn(async move {
            let _ = stdin_handle.forward_channel().await;
        })
        .detach();
    }

    // Send data through stdin
    tx.send("env test input".to_string()).await.unwrap();
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
    assert!(output.contains("env test input"));
    assert!(output.contains("TEST_VAR=test_value"));
}
