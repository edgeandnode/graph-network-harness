//! Base service implementation with manual state management
//!
//! This module provides a base service that manages the lifecycle of a running
//! process or container. Domain-specific services can compose this base service
//! and add their own functionality.

use async_channel::Receiver;
use async_runtime_compat::AsyncSpawner;
use command_executor::event::{ProcessEvent, ProcessEventType};
use service_orchestration::{RunningService, ServiceConfig, ServiceManager};
use std::sync::Arc;

use crate::Error;
use std::result::Result;

/// Base service lifecycle states
#[derive(Debug, Clone, PartialEq)]
pub enum BaseServiceState {
    /// Service is not running
    Stopped,
    /// Service is starting up
    Starting,
    /// Service is running normally
    Running,
    /// Service is shutting down
    Stopping,
    /// Service failed to start or crashed
    Failed { reason: String },
}

/// Commands that can be sent to control the service
#[derive(Debug, Clone)]
pub enum ServiceCommand {
    /// Start the service
    Start,
    /// Stop the service
    Stop,
    /// Restart the service
    Restart,
}

/// Base service that manages lifecycle
///
/// This struct does not handle internal mutability - the owner should wrap it
/// in appropriate concurrency primitives (Arc<Mutex<>>, etc.) as needed.
pub struct BaseService {
    /// Service name
    name: String,
    /// Information about the running service
    running: RunningService,
    /// Stream of events from the process
    events: Receiver<ProcessEvent>,
    /// Service manager for lifecycle operations
    manager: Arc<ServiceManager>,
    /// Service configuration
    config: ServiceConfig,
    /// Current state
    state: BaseServiceState,
}

impl BaseService {
    /// Create a new BaseService from an already-launched service
    pub fn from_running(
        name: String,
        running: RunningService,
        events: Receiver<ProcessEvent>,
        manager: Arc<ServiceManager>,
        config: ServiceConfig,
    ) -> Self {
        Self {
            name,
            running,
            events,
            manager,
            config,
            state: BaseServiceState::Starting,
        }
    }

    /// Create a new BaseService in stopped state
    pub fn new(name: String, config: ServiceConfig, manager: Arc<ServiceManager>) -> Self {
        // Create placeholder RunningService for stopped state
        let running = RunningService::new(name.clone(), config.clone());
        let (_tx, rx) = async_channel::unbounded();

        Self {
            name,
            running,
            events: rx,
            manager,
            config,
            state: BaseServiceState::Stopped,
        }
    }

    /// Handle user commands
    pub async fn command(
        &mut self,
        cmd: ServiceCommand,
        spawner: AsyncSpawner,
    ) -> Result<(), Error> {
        match (&self.state, &cmd) {
            (BaseServiceState::Stopped, ServiceCommand::Start) => {
                tracing::info!("Starting service {}", self.name);

                let (events, running) = self
                    .manager
                    .launch_service(&self.name, self.config.clone(), &spawner)
                    .await
                    .map_err(Error::service_orchestration)?;

                self.running = running;
                self.events = events;
                self.state = BaseServiceState::Starting;
                Ok(())
            }
            (BaseServiceState::Running, ServiceCommand::Stop) => {
                tracing::info!("Stopping service {}", self.name);

                self.manager
                    .stop_service(&self.name, &spawner)
                    .await
                    .map_err(Error::service_orchestration)?;

                self.state = BaseServiceState::Stopping;
                Ok(())
            }
            (BaseServiceState::Running, ServiceCommand::Restart) => {
                // Stop first
                tracing::info!("Restarting service {}", self.name);

                self.manager
                    .stop_service(&self.name, &spawner)
                    .await
                    .map_err(Error::service_orchestration)?;

                self.state = BaseServiceState::Stopping;
                // Note: Will need to wait for stop to complete before starting again
                // This would be handled by monitoring the state and issuing Start when Stopped
                Ok(())
            }
            (BaseServiceState::Failed { .. }, ServiceCommand::Start) => {
                // Allow restart from failed state - transition to stopped first
                self.state = BaseServiceState::Stopped;
                // Then try to start
                tracing::info!("Restarting service {} from failed state", self.name);

                let (events, running) = self
                    .manager
                    .launch_service(&self.name, self.config.clone(), &spawner)
                    .await
                    .map_err(Error::service_orchestration)?;

                self.running = running;
                self.events = events;
                self.state = BaseServiceState::Starting;
                Ok(())
            }
            _ => Err(Error::daemon(format!(
                "Cannot execute {:?} in {:?} state",
                cmd, self.state
            ))),
        }
    }

    /// Process a single event and update state accordingly
    pub fn process_event(&mut self, event: &ProcessEvent) {
        tracing::debug!("Service {} received event: {:?}", self.name, event);

        let new_state = match (&self.state, &event.event_type) {
            (BaseServiceState::Starting, ProcessEventType::Started { pid }) => {
                tracing::info!("Service {} started with PID {}", self.name, pid);
                self.running.pid = Some(*pid);
                Some(BaseServiceState::Running)
            }
            (BaseServiceState::Running, ProcessEventType::Exited { code, .. }) => {
                if let Some(exit_code) = code {
                    if *exit_code == 0 {
                        tracing::info!("Service {} exited normally", self.name);
                        Some(BaseServiceState::Stopped)
                    } else {
                        tracing::error!("Service {} exited with code {}", self.name, exit_code);
                        Some(BaseServiceState::Failed {
                            reason: format!("Exit code: {}", exit_code),
                        })
                    }
                } else {
                    tracing::info!("Service {} terminated by signal", self.name);
                    Some(BaseServiceState::Failed {
                        reason: "Terminated by signal".to_string(),
                    })
                }
            }
            (BaseServiceState::Stopping, ProcessEventType::Exited { .. }) => {
                tracing::info!("Service {} stopped", self.name);
                Some(BaseServiceState::Stopped)
            }
            _ => None,
        };

        if let Some(new_state) = new_state {
            self.state = new_state;
        }
    }

    /// Get the current state
    pub fn state(&self) -> &BaseServiceState {
        &self.state
    }

    /// Get the event stream receiver
    ///
    /// The caller can use this to receive events and call process_event()
    /// to update the service state accordingly
    pub fn events(&self) -> &Receiver<ProcessEvent> {
        &self.events
    }

    /// Get running service info
    pub fn running_info(&self) -> &RunningService {
        &self.running
    }

    /// Get service name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Check if service is running
    pub fn is_running(&self) -> bool {
        matches!(self.state, BaseServiceState::Running)
    }

    /// Check if service is stopped
    pub fn is_stopped(&self) -> bool {
        matches!(self.state, BaseServiceState::Stopped)
    }
}
