//! Main executor type that wraps different launchers

use crate::command::Command;
use crate::error::Result;
use crate::event::LogFilter;
use crate::launcher::Launcher;
use crate::process::ExitResult;

/// An executor that can run commands via a specific launcher
pub struct Executor<L: Launcher> {
    /// The service name for logging/identification
    service_name: String,
    /// The launcher implementation
    launcher: L,
    /// Optional log filter
    log_filter: Option<Box<dyn LogFilter>>,
}

impl<L: Launcher> Executor<L> {
    /// Create a new executor with the given launcher
    pub fn new(service_name: String, launcher: L) -> Self {
        Self {
            service_name,
            launcher,
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

    /// Launch a command and return event stream and process handle
    pub async fn launch(
        &self,
        target: &L::Target,
        command: Command,
    ) -> Result<(L::EventStream, L::Handle)> {
        self.launcher.launch(target, command).await
    }

    /// Execute a command and wait for it to complete
    pub async fn execute(&self, target: &L::Target, command: Command) -> Result<ExitResult> {
        self.launcher.execute(target, command).await
    }

    /// Get a reference to the launcher
    pub fn launcher(&self) -> &L {
        &self.launcher
    }
}
