//! Main executor type that wraps different backends

use async_process::Command;
use crate::backend::Backend;
use crate::error::Result;
use crate::event::LogFilter;
use crate::process::ExitStatus;

/// An executor that can run commands via a specific backend
pub struct Executor<B: Backend> {
    /// The service name for logging/identification
    service_name: String,
    /// The backend implementation
    backend: B,
    /// Optional log filter
    log_filter: Option<Box<dyn LogFilter>>,
}

impl<B: Backend> Executor<B> {
    /// Create a new executor with the given backend
    pub fn new(service_name: String, backend: B) -> Self {
        Self {
            service_name,
            backend,
            log_filter: None,
        }
    }
    
    /// Set a log filter for this executor
    pub fn with_log_filter<F: LogFilter + 'static>(mut self, filter: F) -> Self {
        self.log_filter = Some(Box::new(filter));
        self
    }
    
    /// Get the service name
    pub fn service_name(&self) -> &str {
        &self.service_name
    }
    
    /// Spawn a command and return event stream and process handle
    pub async fn spawn(&self, target: &B::Target, command: Command) -> Result<(B::EventStream, B::Handle)> {
        self.backend.spawn(target, command).await
    }
    
    /// Execute a command and wait for it to complete
    pub async fn execute(&self, target: &B::Target, command: Command) -> Result<ExitStatus> {
        self.backend.execute(target, command).await
    }
    
    /// Get a reference to the backend
    pub fn backend(&self) -> &B {
        &self.backend
    }
}

