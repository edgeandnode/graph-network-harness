//! Core traits for service execution patterns.
//!
//! This module defines the fundamental traits that separate different
//! service lifecycle management patterns and event streaming capabilities.

use super::RunningService;
use crate::{Error, config::ServiceConfig, health::HealthStatus};
use async_trait::async_trait;
use command_executor::event::ProcessEvent;
use futures::stream::BoxStream;

/// Event stream from a running service
pub type EventStream = BoxStream<'static, ProcessEvent>;

/// Trait for streaming events from services
///
/// This trait is implemented by both managed and attached services
/// to provide a unified interface for event streaming.
#[async_trait]
pub trait EventStreamable: Send + Sync {
    /// Stream events from the service
    ///
    /// Returns a stream of process events (stdout, stderr, exit, etc.)
    async fn stream_events(
        &self,
        service: &RunningService,
    ) -> std::result::Result<EventStream, Error>;
}

/// Trait for services we spawn and manage
///
/// This trait is for services where we control the full lifecycle:
/// - Local processes we spawn
/// - Docker containers we create
/// - Remote processes we start via SSH
#[async_trait]
pub trait ManagedService: EventStreamable {
    /// Start a new service instance with the given configuration
    ///
    /// This spawns a new process/container and returns information
    /// about the running service.
    async fn start(&self, config: ServiceConfig) -> std::result::Result<RunningService, Error>;

    /// Stop a managed service instance
    ///
    /// This terminates the process/container we previously started.
    async fn stop(&self, service: &RunningService) -> std::result::Result<(), Error>;

    /// Check if this executor can handle the given configuration
    fn can_handle(&self, config: &ServiceConfig) -> bool;
}

/// Trait for services we attach to but don't manage
///
/// This trait is for existing services we connect to:
/// - External databases (PostgreSQL, MySQL)
/// - Running Docker containers
/// - Services on remote machines we don't control
/// - Third-party APIs
#[async_trait]
pub trait AttachedService: EventStreamable {
    /// Attach to an existing service
    ///
    /// This connects to a running service and returns information
    /// about it. The service must already be running.
    async fn attach(&self, config: ServiceConfig) -> std::result::Result<RunningService, Error>;

    /// Detach from the service
    ///
    /// This disconnects from the service but does NOT stop it.
    /// The service continues running after detachment.
    async fn detach(&self, service: &RunningService) -> std::result::Result<(), Error>;

    /// Check if the attached service is still accessible
    ///
    /// This performs a connectivity check without full health checking.
    async fn is_accessible(&self, service: &RunningService) -> std::result::Result<bool, Error>;

    /// Check if this attacher can handle the given configuration
    fn can_handle(&self, config: &ServiceConfig) -> bool;
}
