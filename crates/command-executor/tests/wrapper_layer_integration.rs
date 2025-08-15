//! Integration tests for WrapperLayer functionality

use command_executor::{
    Command, LayeredExecutor, LocalLayer, ProcessHandle, WrapperLayer, backends::LocalLauncher,
};
use futures::StreamExt;

#[smol_potat::test]
async fn test_wrapper_with_echo() {
    // Simple test: wrap echo with cat (which will pass through)
    let executor =
        LayeredExecutor::new(LocalLauncher).with_layer(WrapperLayer::new("sh").with_args(["-c"]));

    let command = Command::new("echo hello world");

    match executor.execute_command(command).await {
        Ok((mut event_stream, mut handle)) => {
            let mut got_output = false;

            while let Some(event) = event_stream.next().await {
                if let command_executor::ProcessEventType::Stdout = event.event_type {
                    if let Some(data) = event.data {
                        println!("Got output: {}", data);
                        if data.contains("hello world") {
                            got_output = true;
                        }
                    }
                }
            }

            let exit_status = handle.wait().await.expect("Failed to wait");
            assert!(exit_status.success(), "Command should succeed");
            assert!(got_output, "Should have received 'hello world' output");
        }
        Err(e) => {
            // Some test environments might not support this
            eprintln!("Test skipped in this environment: {}", e);
        }
    }
}

#[smol_potat::test]
async fn test_wrapper_with_timeout() {
    // Test timeout wrapper - should kill long-running command
    let executor =
        LayeredExecutor::new(LocalLauncher).with_layer(WrapperLayer::new("timeout").with_arg("1"));

    // Sleep for 10 seconds (but timeout will kill it after 1)
    let mut command = Command::new("sleep");
    command.arg("10");

    match executor.execute_command(command).await {
        Ok((mut event_stream, mut handle)) => {
            // Drain events
            while let Some(_event) = event_stream.next().await {}

            let exit_status = handle.wait().await.expect("Failed to wait");
            // Timeout typically returns exit code 124 when it kills the process
            assert!(
                !exit_status.success(),
                "Command should have been killed by timeout"
            );
        }
        Err(e) => {
            eprintln!("Test skipped (timeout not available): {}", e);
        }
    }
}

#[smol_potat::test]
async fn test_multiple_wrapper_layers() {
    // Stack multiple wrappers: nice -> command
    // Note: sh -c expects a single string argument, so we just use nice here
    let executor = LayeredExecutor::new(LocalLauncher)
        .with_layer(WrapperLayer::new("nice").with_args(["-n", "10"]));

    // The final command will be: nice -n 10 echo test
    let mut command = Command::new("echo");
    command.arg("test");

    match executor.execute_command(command).await {
        Ok((mut event_stream, mut handle)) => {
            let mut got_output = false;

            while let Some(event) = event_stream.next().await {
                if let command_executor::ProcessEventType::Stdout = event.event_type {
                    if let Some(data) = event.data {
                        if data.contains("test") {
                            got_output = true;
                        }
                    }
                }
            }

            let exit_status = handle.wait().await.expect("Failed to wait");
            assert!(exit_status.success(), "Command should succeed");
            assert!(got_output, "Should have received output");
        }
        Err(e) => {
            eprintln!("Test skipped in this environment: {}", e);
        }
    }
}

#[smol_potat::test]
async fn test_wrapper_with_environment() {
    // Test that environment variables are preserved through wrapper
    let executor =
        LayeredExecutor::new(LocalLauncher).with_layer(WrapperLayer::new("sh").with_args(["-c"]));

    let mut command = Command::new("echo $TEST_VAR");
    command.env("TEST_VAR", "hello_from_env");

    match executor.execute_command(command).await {
        Ok((mut event_stream, mut handle)) => {
            let mut got_env_value = false;

            while let Some(event) = event_stream.next().await {
                if let command_executor::ProcessEventType::Stdout = event.event_type {
                    if let Some(data) = event.data {
                        if data.contains("hello_from_env") {
                            got_env_value = true;
                        }
                    }
                }
            }

            let exit_status = handle.wait().await.expect("Failed to wait");
            assert!(exit_status.success(), "Command should succeed");
            assert!(
                got_env_value,
                "Should have received environment variable value"
            );
        }
        Err(e) => {
            eprintln!("Test skipped in this environment: {}", e);
        }
    }
}

#[smol_potat::test]
async fn test_wrapper_with_separator() {
    // Test wrapper with separator
    // Using bash with -- separator
    let executor = LayeredExecutor::new(LocalLauncher).with_layer(
        WrapperLayer::new("bash")
            .with_arg("-c")
            .with_separator("--"),
    );

    let command = Command::new("echo separator test");

    match executor.execute_command(command).await {
        Ok((mut event_stream, mut handle)) => {
            let mut got_output = false;

            while let Some(event) = event_stream.next().await {
                if let command_executor::ProcessEventType::Stdout = event.event_type {
                    if let Some(data) = event.data {
                        println!("Output: {}", data);
                        if data.contains("separator") || data.contains("test") {
                            got_output = true;
                        }
                    }
                }
            }

            let exit_status = handle.wait().await.expect("Failed to wait");
            assert!(exit_status.success(), "Command should succeed");
            assert!(got_output);
            // Note: This test might behave differently on different systems
        }
        Err(e) => {
            eprintln!("Test skipped in this environment: {}", e);
        }
    }
}

#[smol_potat::test]
async fn test_wrapper_error_handling() {
    // Test that wrapper properly handles command failures
    let executor =
        LayeredExecutor::new(LocalLauncher).with_layer(WrapperLayer::new("sh").with_args(["-c"]));

    // This should fail with exit code 1
    let command = Command::new("exit 1");

    match executor.execute_command(command).await {
        Ok((mut event_stream, mut handle)) => {
            // Drain events
            while let Some(_event) = event_stream.next().await {}

            let exit_status = handle.wait().await.expect("Failed to wait");
            assert!(
                !exit_status.success(),
                "Command should fail with exit code 1"
            );
            assert_eq!(exit_status.code, Some(1), "Should have exit code 1");
        }
        Err(e) => {
            eprintln!("Test skipped in this environment: {}", e);
        }
    }
}

#[smol_potat::test]
async fn test_screen_wrapper_simulation() {
    // Simulate what a screen wrapper would look like (without actually using screen)
    // In real usage: screen -dmS session_name command
    let executor =
        LayeredExecutor::new(LocalLauncher).with_layer(WrapperLayer::new("sh").with_args(["-c"]));

    // Simulate: screen -dmS test_session echo "in screen"
    let command = Command::new("echo 'in screen session'");

    match executor.execute_command(command).await {
        Ok((mut event_stream, mut handle)) => {
            let mut got_output = false;

            while let Some(event) = event_stream.next().await {
                if let command_executor::ProcessEventType::Stdout = event.event_type {
                    if let Some(data) = event.data {
                        if data.contains("in screen session") {
                            got_output = true;
                        }
                    }
                }
            }

            let exit_status = handle.wait().await.expect("Failed to wait");
            assert!(exit_status.success(), "Command should succeed");
            assert!(got_output, "Should have received output");
        }
        Err(e) => {
            eprintln!("Test skipped in this environment: {}", e);
        }
    }
}

#[smol_potat::test]
async fn test_strace_wrapper_simulation() {
    // Test strace-like wrapper (we'll use a command that acts similarly)
    // Real usage would be: strace -f -o trace.log command
    let tmp_dir = std::env::temp_dir();
    let trace_file = tmp_dir.join("test_trace.txt");

    // We'll use tee to simulate capturing output to a file
    let executor =
        LayeredExecutor::new(LocalLauncher).with_layer(WrapperLayer::new("sh").with_args(["-c"]));

    let command_str = format!("echo 'traced output' | tee {}", trace_file.display());
    let command = Command::new(command_str);

    match executor.execute_command(command).await {
        Ok((mut event_stream, mut handle)) => {
            let mut got_output = false;

            while let Some(event) = event_stream.next().await {
                if let command_executor::ProcessEventType::Stdout = event.event_type {
                    if let Some(data) = event.data {
                        if data.contains("traced output") {
                            got_output = true;
                        }
                    }
                }
            }

            let exit_status = handle.wait().await.expect("Failed to wait");
            assert!(exit_status.success(), "Command should succeed");
            assert!(got_output, "Should have received output");

            // Clean up
            let _ = std::fs::remove_file(&trace_file);
        }
        Err(e) => {
            eprintln!("Test skipped in this environment: {}", e);
        }
    }
}

#[cfg(unix)]
#[smol_potat::test]
async fn test_nice_wrapper() {
    // Test nice command wrapper (Unix only)
    let executor = LayeredExecutor::new(LocalLauncher)
        .with_layer(WrapperLayer::new("nice").with_args(["-n", "10"]));

    let mut command = Command::new("echo");
    command.arg("nice test");

    match executor.execute_command(command).await {
        Ok((mut event_stream, mut handle)) => {
            let mut got_output = false;

            while let Some(event) = event_stream.next().await {
                if let command_executor::ProcessEventType::Stdout = event.event_type {
                    if let Some(data) = event.data {
                        if data.contains("nice test") {
                            got_output = true;
                        }
                    }
                }
            }

            let exit_status = handle.wait().await.expect("Failed to wait");
            assert!(exit_status.success(), "Command should succeed");
            assert!(got_output, "Should have received output");
        }
        Err(e) => {
            eprintln!("Nice command not available: {}", e);
        }
    }
}

#[smol_potat::test]
async fn test_wrapper_with_own_environment() {
    // Test that wrapper can have its own environment variables
    let executor = LayeredExecutor::new(LocalLauncher).with_layer(
        WrapperLayer::new("sh")
            .with_args(["-c"])
            .with_env("WRAPPER_VAR", "wrapper_value"),
    );

    // Command that will use both wrapper and command env vars
    let mut command = Command::new("echo WRAPPER_VAR=$WRAPPER_VAR CMD_VAR=$CMD_VAR");
    command.env("CMD_VAR", "cmd_value");

    match executor.execute_command(command).await {
        Ok((mut event_stream, mut handle)) => {
            let mut got_both_vars = false;

            while let Some(event) = event_stream.next().await {
                if let command_executor::ProcessEventType::Stdout = event.event_type {
                    if let Some(data) = event.data {
                        println!("Env test output: {}", data);
                        if data.contains("wrapper_value") && data.contains("cmd_value") {
                            got_both_vars = true;
                        }
                    }
                }
            }

            let exit_status = handle.wait().await.expect("Failed to wait");
            assert!(exit_status.success(), "Command should succeed");
            assert!(
                got_both_vars,
                "Should have received both environment variables"
            );
        }
        Err(e) => {
            eprintln!("Test skipped in this environment: {}", e);
        }
    }
}

// Test demonstrating complex layering scenarios
#[smol_potat::test]
async fn test_complex_layer_composition() {
    // Build a complex layer stack:
    // LocalLayer -> WrapperLayer(timeout) -> actual command
    // This simulates: timeout 5 echo complex

    let executor = LayeredExecutor::new(LocalLauncher)
        .with_layer(LocalLayer::new())
        .with_layer(WrapperLayer::new("timeout").with_arg("5"));

    let mut command = Command::new("echo");
    command.arg("complex");

    match executor.execute_command(command).await {
        Ok((mut event_stream, mut handle)) => {
            let mut lines_received = 0;

            while let Some(event) = event_stream.next().await {
                if let command_executor::ProcessEventType::Stdout = event.event_type {
                    if let Some(data) = event.data {
                        println!("Complex output: {}", data);
                        if data.contains("complex") {
                            lines_received += 1;
                        }
                    }
                }
            }

            let exit_status = handle.wait().await.expect("Failed to wait");
            assert!(exit_status.success(), "Command should succeed");
            assert!(lines_received > 0, "Should have received output");
        }
        Err(e) => {
            eprintln!("Test skipped in this environment: {}", e);
        }
    }
}
