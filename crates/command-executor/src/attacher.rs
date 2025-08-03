//! Attacher trait for connecting to and controlling existing services
//!
//! This module provides traits for attaching to services that we can control,
//! such as systemd services, docker containers we didn't create, etc.

use crate::error::Result;
use crate::event::ProcessEvent;
use async_trait::async_trait;
use futures::stream::Stream;

/// Configuration for attaching to an existing service
#[derive(Debug, Clone)]
pub struct AttachConfig {
    /// Whether to follow logs from the beginning or only new entries
    pub follow_from_start: bool,
    /// Maximum number of historical log lines to include
    pub history_lines: Option<usize>,
    /// Timeout for attachment operations
    pub timeout_seconds: Option<u64>,
}

impl Default for AttachConfig {
    fn default() -> Self {
        Self {
            follow_from_start: false,
            history_lines: Some(100),
            timeout_seconds: Some(30),
        }
    }
}

/// A handle for monitoring an attached service
#[async_trait]
pub trait AttachedHandle: Send + Sync {
    /// Get the service identifier (name, ID, etc.)
    fn id(&self) -> String;

    /// Check the current status of the service
    async fn status(&self) -> Result<ServiceStatus>;

    /// Disconnect from the service (stop monitoring)
    async fn disconnect(&mut self) -> Result<()>;
}

/// Status of an attached service
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceStatus {
    /// Service is running
    Running,
    /// Service is stopped
    Stopped,
    /// Service failed to start or crashed
    Failed,
    /// Service status is unknown
    Unknown,
}

/// An attacher that can connect to existing services
#[async_trait]
pub trait Attacher: Send + Sync + 'static {
    /// The target configuration type for this attacher
    type Target: Send + Sync;

    /// The event stream type this attacher produces
    type EventStream: Stream<Item = ProcessEvent> + Send + Unpin;

    /// The service handle type this attacher produces
    type Handle: AttachedHandle;

    /// Attach to an existing service, returning event stream and control handle
    async fn attach(
        &self,
        target: &Self::Target,
        config: AttachConfig,
    ) -> Result<(Self::EventStream, Self::Handle)>;
}
