//! Tests for stdin functionality

#[cfg(test)]
mod tests {
    use crate::command::Command;
    use crate::event::ProcessEventType;
    use crate::executor::Executor;
    use crate::process::ProcessHandle;
    use crate::target::Target;
    use futures::StreamExt;

    #[smol_potat::test]
    async fn test_stdin_handle_basic() {
        let executor = Executor::local("test_stdin");
        let target = Target::Command;

        // Test that stdin handle is available when no channel is provided
        let cmd = Command::new("cat");
        let (_events, mut handle) = executor.launch(&target, cmd).await.unwrap();

        assert!(
            handle.stdin_mut().is_some(),
            "Stdin handle should be available"
        );

        // Clean up
        handle.terminate().await.ok();
    }

    #[smol_potat::test]
    async fn test_stdin_write_to_process() {
        // Use tee to write stdin to both stdout and a file
        let temp_file = std::env::temp_dir().join(format!("stdin_test_{}.txt", std::process::id()));
        let mut cmd = Command::new("tee");
        cmd.arg(&temp_file);

        let executor = Executor::local("test_stdin_write");
        let target = Target::Command;

        let (mut events, mut handle) = executor.launch(&target, cmd).await.unwrap();

        // Write to stdin
        if let Some(stdin) = handle.stdin_mut() {
            stdin.write_line("Test line 1").await.unwrap();
            stdin.write_line("Test line 2").await.unwrap();
            stdin.close(); // Close stdin to signal EOF
        }

        // Wait a bit for process to write
        smol::Timer::after(std::time::Duration::from_millis(100)).await;

        // Collect some stdout
        let mut got_output = false;
        for _ in 0..10 {
            if let Some(event) = events.next().await {
                if matches!(event.event_type, ProcessEventType::Stdout) {
                    got_output = true;
                    if let Some(data) = event.data {
                        println!("Got stdout: {}", data);
                    }
                }
            }
        }

        // Clean up process
        handle.terminate().await.ok();

        // Check the file was written
        if let Ok(contents) = std::fs::read_to_string(&temp_file) {
            println!("File contents: {:?}", contents);
            assert!(
                contents.contains("Test line 1"),
                "File should contain test line 1"
            );
            assert!(
                contents.contains("Test line 2"),
                "File should contain test line 2"
            );
        }

        // Clean up file
        std::fs::remove_file(&temp_file).ok();

        assert!(got_output, "Should have received some stdout output");
    }
}
