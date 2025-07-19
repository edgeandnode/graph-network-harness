//! Backend trait for different execution contexts

use async_trait::async_trait;
use async_process::Command;
use futures::stream::Stream;
use crate::error::Result;
use crate::event::ProcessEvent;
use crate::process::{ProcessHandle, ExitStatus};

/// A backend that can execute commands in a specific context
#[async_trait]
pub trait Backend: Send + Sync + 'static {
    /// The target configuration type for this backend
    type Target: Send + Sync;
    
    /// The event stream type this backend produces
    type EventStream: Stream<Item = ProcessEvent> + Send + Unpin;
    
    /// The process handle type this backend produces
    type Handle: ProcessHandle;
    
    /// Spawn a command for the given target, returning event stream and control handle
    async fn spawn(&self, target: &Self::Target, command: Command) -> Result<(Self::EventStream, Self::Handle)>;
    
    /// Execute a command and wait for it to complete
    async fn execute(&self, target: &Self::Target, command: Command) -> Result<ExitStatus> {
        let (_events, mut handle) = self.spawn(target, command).await?;
        handle.wait().await
    }
}