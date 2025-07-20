//! Launcher trait for executing commands in different contexts

use async_trait::async_trait;
use futures::stream::Stream;
use crate::command::Command;
use crate::error::Result;
use crate::event::ProcessEvent;
use crate::process::{ProcessHandle, ExitStatus};

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
    async fn launch(&self, target: &Self::Target, command: Command) -> Result<(Self::EventStream, Self::Handle)>;
    
    /// Execute a command and wait for it to complete
    async fn execute(&self, target: &Self::Target, command: Command) -> Result<ExitStatus> {
        let (_events, mut handle) = self.launch(target, command).await?;
        handle.wait().await
    }
}