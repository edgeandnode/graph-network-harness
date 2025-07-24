//! Launcher trait for executing commands in different contexts

use crate::command::Command;
use crate::error::Result;
use crate::event::ProcessEvent;
use crate::process::{ExitResult, ProcessHandle};
use async_trait::async_trait;
use futures::stream::Stream;

/// A launcher that can execute commands in a specific context
#[async_trait]
pub trait Launcher: Send + Sync + 'static {
    /// The target configuration type for this launcher
    type Target: Send + Sync;

    /// The event stream type this launcher produces
    type EventStream: Stream<Item = ProcessEvent> + Send + Unpin;

    /// The process handle type this launcher produces
    type Handle: ProcessHandle;

    /// Launch a command for the given target, returning event stream and control handle
    async fn launch(
        &self,
        target: &Self::Target,
        command: Command,
    ) -> Result<(Self::EventStream, Self::Handle)>;

    /// Execute a command and wait for it to complete, capturing output
    async fn execute(&self, target: &Self::Target, command: Command) -> Result<ExitResult> {
        use futures::StreamExt;

        let (mut events, mut handle) = self.launch(target, command).await?;
        let mut output = String::new();

        // Collect all output from the event stream
        while let Some(event) = events.next().await {
            if let Some(data) = &event.data {
                output.push_str(data);
                output.push('\n');
            }
        }

        let status = handle.wait().await?;
        Ok(ExitResult { status, output })
    }
}
