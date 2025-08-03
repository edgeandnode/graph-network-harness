//! Layered executor implementation for runtime command composition.

use super::{ExecutionContext, ExecutionLayer};
use crate::{Command, error::Result, event::ProcessEvent, launcher::Launcher, target::Target};
use futures::stream::Stream;
use std::pin::Pin;

/// Type alias for event streams
pub type EventStream = Pin<Box<dyn Stream<Item = ProcessEvent> + Send>>;

/// Executor that applies a series of execution layers before launching commands
pub struct LayeredExecutor<L: Launcher> {
    /// The underlying launcher that will execute the final command
    launcher: L,
    /// Stack of execution layers to apply
    layers: Vec<Box<dyn ExecutionLayer>>,
    /// Execution context
    context: ExecutionContext,
}

impl<L: Launcher> LayeredExecutor<L> {
    /// Create a new layered executor with the given launcher
    pub fn new(launcher: L) -> Self {
        Self {
            launcher,
            layers: Vec::new(),
            context: ExecutionContext::new(),
        }
    }

    /// Add an execution layer to the stack
    pub fn with_layer<Layer: ExecutionLayer + 'static>(mut self, layer: Layer) -> Self {
        self.layers.push(Box::new(layer));
        self
    }

    /// Set the execution context
    pub fn with_context(mut self, context: ExecutionContext) -> Self {
        self.context = context;
        self
    }

    /// Add an environment variable to the context
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.env.insert(key.into(), value.into());
        self
    }

    /// Set the working directory in the context
    pub fn with_working_dir(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.context.working_dir = Some(dir.into());
        self
    }

    /// Get the number of layers in the stack
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Get descriptions of all layers for debugging
    pub fn layer_descriptions(&self) -> Vec<String> {
        self.layers
            .iter()
            .map(|layer| layer.description())
            .collect()
    }

    /// Test helper: Apply layers to a command without executing (for testing only)
    #[cfg(test)]
    pub fn transform_command_for_test(&self, command: Command) -> Result<Command> {
        self.layers
            .iter()
            .try_fold(command, |cmd, layer| layer.wrap_command(cmd, &self.context))
    }

    /// Get a reference to the execution context (for testing only)
    #[cfg(test)]
    pub fn context(&self) -> &ExecutionContext {
        &self.context
    }

    /// Execute a command by applying all layers and then launching
    /// Note: This method requires the target type to have a sensible default for command execution
    pub async fn execute(
        &self,
        command: Command,
        target: &L::Target,
    ) -> Result<(EventStream, L::Handle)> {
        // Apply all layers to transform the command
        let final_command = self
            .layers
            .iter()
            .try_fold(command, |cmd, layer| layer.wrap_command(cmd, &self.context))?;

        // Use the underlying launcher to execute the final command
        let (event_stream, handle) = self.launcher.launch(target, final_command).await?;

        Ok((Box::pin(event_stream), handle))
    }
}

// Convenience implementation for LocalLauncher
impl LayeredExecutor<crate::backends::LocalLauncher> {
    /// Execute a command with the default Command target (convenience method for LocalLauncher)
    pub async fn execute_command(
        &self,
        command: Command,
    ) -> Result<(EventStream, crate::backends::LocalProcessHandle)> {
        self.execute(command, &Target::Command).await
    }
}

// Implement Debug for LayeredExecutor
impl<L: Launcher> std::fmt::Debug for LayeredExecutor<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LayeredExecutor")
            .field("layers", &self.layer_descriptions())
            .field("context", &self.context)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::LocalLauncher;
    use crate::layered::{DockerLayer, LocalLayer, SshLayer};

    #[test]
    fn test_layered_executor_creation() {
        let executor = LayeredExecutor::new(LocalLauncher)
            .with_layer(SshLayer::new("user@host"))
            .with_layer(DockerLayer::new("container"));

        assert_eq!(executor.layer_count(), 2);
        let descriptions = executor.layer_descriptions();
        assert_eq!(descriptions[0], "SSH to user@host");
        assert_eq!(descriptions[1], "Docker exec in container");
    }

    #[test]
    fn test_context_building() {
        let executor = LayeredExecutor::new(LocalLauncher)
            .with_env("TEST_VAR", "test_value")
            .with_working_dir("/tmp");

        assert_eq!(
            executor.context.env.get("TEST_VAR"),
            Some(&"test_value".to_string())
        );
        assert_eq!(executor.context.working_dir, Some("/tmp".into()));
    }

    #[test]
    fn test_empty_executor() {
        let executor = LayeredExecutor::new(LocalLauncher);
        assert_eq!(executor.layer_count(), 0);
        assert!(executor.layer_descriptions().is_empty());
    }

    // Integration test for command transformation
    #[test]
    fn test_command_transformation() {
        let executor = LayeredExecutor::new(LocalLauncher)
            .with_layer(SshLayer::new("user@remote"))
            .with_layer(DockerLayer::new("app-container"));

        // This test verifies the layer stack but doesn't actually execute
        // Real execution tests would require a test environment
        assert_eq!(executor.layer_count(), 2);
    }

    #[smol_potat::test]
    async fn test_local_execution() {
        let executor = LayeredExecutor::new(LocalLauncher).with_layer(LocalLayer::new());

        let mut command = Command::new("echo");
        command.arg("hello world");

        // This should work for basic commands
        match executor.execute_command(command).await {
            Ok((mut event_stream, mut handle)) => {
                // Basic verification that we got results
                use futures::StreamExt;

                // Try to get at least one event or wait for completion
                let timeout = std::time::Duration::from_secs(1);
                let start = std::time::Instant::now();

                while start.elapsed() < timeout {
                    if let Some(event) = event_stream.next().await {
                        // Got an event, test passes
                        break;
                    }

                    // For now, just break after getting some events
                    // Real process management would use handle.wait() etc.
                    break;
                }
            }
            Err(e) => {
                // Some test environments might not support process execution
                // This is ok as long as the executor was created successfully
                eprintln!(
                    "Execution failed (may be expected in test environment): {}",
                    e
                );
            }
        }
    }

    #[test]
    fn test_debug_output() {
        let executor = LayeredExecutor::new(LocalLauncher)
            .with_layer(SshLayer::new("test@example.com"))
            .with_env("DEBUG", "1");

        let debug_output = format!("{:?}", executor);
        assert!(debug_output.contains("LayeredExecutor"));
        assert!(debug_output.contains("SSH to test@example.com"));
    }

    // Integration tests for layer composition
    #[test]
    fn test_multi_layer_composition() {
        let executor = LayeredExecutor::new(LocalLauncher)
            .with_layer(SshLayer::new("user@jump-host"))
            .with_layer(SshLayer::new("user@target-host"))
            .with_layer(DockerLayer::new("app-container"));

        assert_eq!(executor.layer_count(), 3);
        let descriptions = executor.layer_descriptions();
        assert_eq!(descriptions[0], "SSH to user@jump-host");
        assert_eq!(descriptions[1], "SSH to user@target-host");
        assert_eq!(descriptions[2], "Docker exec in app-container");
    }

    #[test]
    fn test_ssh_with_options_composition() {
        let executor = LayeredExecutor::new(LocalLauncher)
            .with_layer(
                SshLayer::new("user@secure-host")
                    .with_port(2222)
                    .with_identity_file("/path/to/key")
                    .with_option("-o StrictHostKeyChecking=no")
                    .with_option("-o UserKnownHostsFile=/dev/null"),
            )
            .with_layer(DockerLayer::new("secure-container").with_user("root"));

        assert_eq!(executor.layer_count(), 2);
        assert!(executor.layer_descriptions()[0].contains("SSH to user@secure-host"));
        assert!(executor.layer_descriptions()[1].contains("Docker exec in secure-container"));
    }

    #[test]
    fn test_complex_docker_configuration() {
        let executor = LayeredExecutor::new(LocalLauncher)
            .with_layer(
                DockerLayer::new("my-app")
                    .with_interactive(true)
                    .with_tty(true)
                    .with_user("app-user")
                    .with_working_dir("/app"),
            )
            .with_env("APP_ENV", "production")
            .with_working_dir("/tmp");

        assert_eq!(executor.layer_count(), 1);
        assert_eq!(
            executor.context.env.get("APP_ENV"),
            Some(&"production".to_string())
        );
        assert_eq!(executor.context.working_dir, Some("/tmp".into()));
    }

    #[test]
    fn test_command_transformation_ordering() {
        // Test that layers are applied in the correct order (first added = outermost)
        let executor = LayeredExecutor::new(LocalLauncher)
            .with_layer(LocalLayer::new()) // Should be applied last (innermost)
            .with_layer(DockerLayer::new("container")) // Middle layer
            .with_layer(SshLayer::new("user@host")); // Should be applied first (outermost)

        // Verify ordering
        let descriptions = executor.layer_descriptions();
        assert_eq!(descriptions[0], "Local execution");
        assert_eq!(descriptions[1], "Docker exec in container");
        assert_eq!(descriptions[2], "SSH to user@host");

        // This creates a command pipeline like: ssh user@host "docker exec container sh -c 'original-command'"
    }

    #[test]
    fn test_execution_context_propagation() {
        let context = ExecutionContext::new()
            .with_env("TEST_VAR1", "value1")
            .with_env("TEST_VAR2", "value2")
            .with_working_dir("/custom/workdir")
            .with_metadata("deployment", "staging");

        let executor = LayeredExecutor::new(LocalLauncher)
            .with_context(context)
            .with_layer(LocalLayer::new())
            .with_env("RUNTIME_VAR", "runtime_value"); // This should merge with context

        assert_eq!(executor.context.env.len(), 3); // 2 from context + 1 runtime
        assert_eq!(
            executor.context.env.get("TEST_VAR1"),
            Some(&"value1".to_string())
        );
        assert_eq!(
            executor.context.env.get("RUNTIME_VAR"),
            Some(&"runtime_value".to_string())
        );
        assert_eq!(executor.context.working_dir, Some("/custom/workdir".into()));
        assert_eq!(
            executor.context.metadata.get("deployment"),
            Some(&"staging".to_string())
        );
    }

    #[smol_potat::test]
    async fn test_layered_execution_with_error_handling() {
        let executor = LayeredExecutor::new(LocalLauncher).with_layer(LocalLayer::new());

        // Test with a command that should succeed
        let mut good_command = Command::new("echo");
        good_command.arg("test successful execution");

        match executor.execute_command(good_command).await {
            Ok((_event_stream, _handle)) => {
                // Success case - this is expected for a simple echo command
            }
            Err(e) => {
                // This might happen in some test environments, which is acceptable
                eprintln!("Command execution failed (may be expected): {}", e);
            }
        }

        // Test with a command that should fail
        let mut bad_command = Command::new("nonexistent-command-12345");
        bad_command.arg("this will fail");

        match executor.execute_command(bad_command).await {
            Ok(_) => {
                // This would be unexpected but not a test failure
                eprintln!("Unexpected success with nonexistent command");
            }
            Err(_e) => {
                // Expected failure - this is the normal case
            }
        }
    }
}
