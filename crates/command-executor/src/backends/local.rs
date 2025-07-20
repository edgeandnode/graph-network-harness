//! Local process execution backend

use async_process::{Child, Command as AsyncCommand, Stdio};
use async_trait::async_trait;
use futures::stream::Stream;
use futures_lite::io::{AsyncBufReadExt, BufReader, Lines};
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::command::Command as ServiceCommand;
use crate::error::{Error, Result};
use crate::event::{LogFilter, LogSource, NoOpFilter, ProcessEvent, ProcessEventType};
use crate::launcher::{AttachConfig, AttachedHandle, Attacher, Launcher, ServiceStatus};
use crate::process::{ExitStatus, ProcessHandle};

// Local target types

/// Execute as a one-off command
#[derive(Debug, Clone)]
pub struct Command {
    // Empty for now, but can be expanded later
}

/// Execute as a managed process (we track PID and lifecycle)
#[derive(Debug, Clone)]
pub struct ManagedProcess {
    /// Optional process group ID for managing child processes
    process_group: Option<i32>,
    /// Whether to restart on failure
    restart_on_failure: bool,
}

impl ManagedProcess {
    /// Create a new managed process with default settings
    pub fn new() -> Self {
        Self {
            process_group: None,
            restart_on_failure: false,
        }
    }
    
    /// Create a builder for more complex configurations
    pub fn builder() -> ManagedProcessBuilder {
        ManagedProcessBuilder::new()
    }
    
    /// Set the process group ID
    pub fn with_process_group(mut self, pgid: i32) -> Self {
        self.process_group = Some(pgid);
        self
    }

    /// Enable restart on failure
    pub fn with_restart_on_failure(mut self) -> Self {
        self.restart_on_failure = true;
        self
    }
}

/// Builder for ManagedProcess
pub struct ManagedProcessBuilder {
    process_group: Option<i32>,
    restart_on_failure: bool,
}

impl ManagedProcessBuilder {
    /// Create a new builder
    fn new() -> Self {
        Self {
            process_group: None,
            restart_on_failure: false,
        }
    }
    
    /// Set the process group ID
    pub fn process_group(mut self, pgid: i32) -> Self {
        self.process_group = Some(pgid);
        self
    }
    
    /// Enable restart on failure
    pub fn restart_on_failure(mut self, enabled: bool) -> Self {
        self.restart_on_failure = enabled;
        self
    }
    
    /// Build the ManagedProcess
    pub fn build(self) -> ManagedProcess {
        ManagedProcess {
            process_group: self.process_group,
            restart_on_failure: self.restart_on_failure,
        }
    }
}

/// Execute via systemd (systemctl commands)
#[derive(Debug, Clone)]
pub struct SystemdService {
    /// The systemd unit name
    unit_name: String,
}

impl SystemdService {
    /// Create a new systemd service target
    pub fn new(unit_name: impl Into<String>) -> Self {
        Self {
            unit_name: unit_name.into(),
        }
    }

    /// Get the unit name
    pub fn unit_name(&self) -> &str {
        &self.unit_name
    }
}

/// Execute via systemd-portable (portablectl commands)
#[derive(Debug, Clone)]
pub struct SystemdPortable {
    /// The portable service image name
    image_name: String,
    /// The systemd unit name
    unit_name: String,
}

impl SystemdPortable {
    /// Create a new systemd-portable service target
    pub fn new(image_name: impl Into<String>, unit_name: impl Into<String>) -> Self {
        Self {
            image_name: image_name.into(),
            unit_name: unit_name.into(),
        }
    }

    /// Get the image name
    pub fn image_name(&self) -> &str {
        &self.image_name
    }

    /// Get the unit name
    pub fn unit_name(&self) -> &str {
        &self.unit_name
    }
}

/// A generic managed service with configurable commands
#[derive(Debug, Clone)]
pub struct ManagedService {
    /// Service identifier
    name: String,
    /// How to check if service is running
    status_command: ServiceCommand,
    /// How to start the service
    start_command: ServiceCommand,
    /// How to stop the service
    stop_command: ServiceCommand,
    /// How to restart the service (optional, will use stop+start if not provided)
    restart_command: Option<ServiceCommand>,
    /// How to reload the service (optional)
    reload_command: Option<ServiceCommand>,
    /// How to tail the logs
    log_command: ServiceCommand,
}

impl ManagedService {
    /// Create a builder for a managed service
    pub fn builder(name: impl Into<String>) -> ManagedServiceBuilder {
        ManagedServiceBuilder::new(name)
    }

    /// Get the service name
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Builder for ManagedService
pub struct ManagedServiceBuilder {
    name: String,
    status_command: Option<ServiceCommand>,
    start_command: Option<ServiceCommand>,
    stop_command: Option<ServiceCommand>,
    restart_command: Option<ServiceCommand>,
    reload_command: Option<ServiceCommand>,
    log_command: Option<ServiceCommand>,
}

impl ManagedServiceBuilder {
    /// Create a new builder
    fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status_command: None,
            start_command: None,
            stop_command: None,
            restart_command: None,
            reload_command: None,
            log_command: None,
        }
    }

    /// Set the status command
    pub fn status_command(mut self, command: ServiceCommand) -> Self {
        self.status_command = Some(command);
        self
    }

    /// Set the start command
    pub fn start_command(mut self, command: ServiceCommand) -> Self {
        self.start_command = Some(command);
        self
    }

    /// Set the stop command
    pub fn stop_command(mut self, command: ServiceCommand) -> Self {
        self.stop_command = Some(command);
        self
    }

    /// Set the restart command (optional)
    pub fn restart_command(mut self, command: ServiceCommand) -> Self {
        self.restart_command = Some(command);
        self
    }

    /// Set the reload command (optional)
    pub fn reload_command(mut self, command: ServiceCommand) -> Self {
        self.reload_command = Some(command);
        self
    }

    /// Set the log command
    pub fn log_command(mut self, command: ServiceCommand) -> Self {
        self.log_command = Some(command);
        self
    }

    /// Build the ManagedService
    pub fn build(self) -> Result<ManagedService> {
        Ok(ManagedService {
            name: self.name,
            status_command: self
                .status_command
                .ok_or_else(|| Error::spawn_failed("status_command is required"))?,
            start_command: self
                .start_command
                .ok_or_else(|| Error::spawn_failed("start_command is required"))?,
            stop_command: self
                .stop_command
                .ok_or_else(|| Error::spawn_failed("stop_command is required"))?,
            restart_command: self.restart_command,
            reload_command: self.reload_command,
            log_command: self
                .log_command
                .ok_or_else(|| Error::spawn_failed("log_command is required"))?,
        })
    }
}

/// Target types supported by the local backend
#[derive(Debug, Clone)]
pub enum LocalTarget {
    /// One-off command
    Command(Command),
    /// Managed process
    ManagedProcess(ManagedProcess),
    /// Systemd service
    SystemdService(SystemdService),
    /// Systemd-portable service
    SystemdPortable(SystemdPortable),
}

/// Launcher for executing processes locally
#[derive(Debug, Clone, Copy)]
pub struct LocalLauncher;

/// A handle to control a local process
pub struct LocalProcessHandle {
    /// The underlying child process
    child: Child,
    /// Whether to kill the process on drop
    kill_on_drop: bool,
}

/// Stream of process events
pub struct ProcessEventStream {
    _service_name: String,
    stdout: Option<Lines<BufReader<async_process::ChildStdout>>>,
    stderr: Option<Lines<BufReader<async_process::ChildStderr>>>,
    filter: Box<dyn LogFilter + Send>,
    started_sent: bool,
    child_id: u32,
}

#[async_trait]
impl Launcher for LocalLauncher {
    type Target = LocalTarget;
    type EventStream = ProcessEventStream;
    type Handle = LocalProcessHandle;

    async fn launch(
        &self,
        target: &Self::Target,
        mut command: AsyncCommand,
    ) -> Result<(Self::EventStream, Self::Handle)> {
        match target {
            LocalTarget::Command(_) | LocalTarget::ManagedProcess(_) => {
                // Configure stdio for streaming
                command.stdout(Stdio::piped());
                command.stderr(Stdio::piped());

                let mut child = command
                    .spawn()
                    .map_err(|e| Error::spawn_failed(format!("Failed to spawn process: {}", e)))?;

                let child_id = child.id();

                let stdout = child.stdout.take().map(|s| BufReader::new(s).lines());
                let stderr = child.stderr.take().map(|s| BufReader::new(s).lines());

                // TODO: Get service name from elsewhere (passed from Executor?)
                let service_name = "local_process".to_string();

                let events = ProcessEventStream {
                    _service_name: service_name,
                    stdout,
                    stderr,
                    filter: Box::new(NoOpFilter),
                    started_sent: false,
                    child_id,
                };

                let handle = LocalProcessHandle {
                    child,
                    kill_on_drop: true,
                };

                Ok((events, handle))
            }

            LocalTarget::SystemdService(_service) => {
                // TODO: Use systemctl to manage the service
                Err(Error::spawn_failed("SystemdService not yet implemented"))
            }

            LocalTarget::SystemdPortable(portable) => {
                // For systemd-portable, the incoming command is expected to be a portablectl command
                // We simply execute it as-is, since the user knows what they want to do

                // The command passed should already be configured appropriately
                // e.g., AsyncCommand::new("portablectl").arg("attach").arg("--enable").arg("--now").arg("myapp.raw")

                // Configure stdio for streaming
                command.stdout(Stdio::piped());
                command.stderr(Stdio::piped());

                let mut child = command.spawn().map_err(|e| {
                    Error::spawn_failed(format!("Failed to spawn portablectl command: {}", e))
                })?;

                let child_id = child.id();

                let stdout = child.stdout.take().map(|s| BufReader::new(s).lines());
                let stderr = child.stderr.take().map(|s| BufReader::new(s).lines());

                let service_name = portable.unit_name().to_string();

                let events = ProcessEventStream {
                    _service_name: service_name,
                    stdout,
                    stderr,
                    filter: Box::new(NoOpFilter),
                    started_sent: false,
                    child_id,
                };

                let handle = LocalProcessHandle {
                    child,
                    kill_on_drop: true,
                };

                Ok((events, handle))
            }
        }
    }
}

#[async_trait]
impl ProcessHandle for LocalProcessHandle {
    fn pid(&self) -> Option<u32> {
        Some(self.child.id())
    }

    async fn wait(&mut self) -> Result<ExitStatus> {
        let status = self
            .child
            .status()
            .await
            .map_err(|e| Error::spawn_failed(format!("Failed to wait for process: {}", e)))?;

        Ok(ExitStatus {
            code: status.code(),
            #[cfg(unix)]
            signal: {
                use std::os::unix::process::ExitStatusExt;
                status.signal()
            },
        })
    }

    async fn terminate(&mut self) -> Result<()> {
        #[cfg(unix)]
        {
            use nix::sys::signal::{self, Signal};
            use nix::unistd::Pid;

            let pid = Pid::from_raw(self.child.id() as i32);
            signal::kill(pid, Signal::SIGTERM)
                .map_err(|e| Error::signal_failed(15, e.to_string()))?;
        }

        #[cfg(not(unix))]
        {
            self.child
                .kill()
                .map_err(|e| Error::signal_failed(-1, e.to_string()))?;
        }

        Ok(())
    }

    async fn kill(&mut self) -> Result<()> {
        #[cfg(unix)]
        {
            use nix::sys::signal::{self, Signal};
            use nix::unistd::Pid;

            let pid = Pid::from_raw(self.child.id() as i32);
            signal::kill(pid, Signal::SIGKILL)
                .map_err(|e| Error::signal_failed(9, e.to_string()))?;
        }

        #[cfg(not(unix))]
        {
            self.child
                .kill()
                .map_err(|e| Error::signal_failed(-1, e.to_string()))?;
        }

        Ok(())
    }

    async fn interrupt(&mut self) -> Result<()> {
        #[cfg(unix)]
        {
            use nix::sys::signal::{self, Signal};
            use nix::unistd::Pid;

            let pid = Pid::from_raw(self.child.id() as i32);
            signal::kill(pid, Signal::SIGINT)
                .map_err(|e| Error::signal_failed(2, e.to_string()))?;
        }

        #[cfg(not(unix))]
        {
            // Windows doesn't have SIGINT equivalent
            self.terminate().await?;
        }

        Ok(())
    }

    async fn reload(&mut self) -> Result<()> {
        #[cfg(unix)]
        {
            use nix::sys::signal::{self, Signal};
            use nix::unistd::Pid;

            let pid = Pid::from_raw(self.child.id() as i32);
            signal::kill(pid, Signal::SIGHUP)
                .map_err(|e| Error::signal_failed(1, e.to_string()))?;
        }

        #[cfg(not(unix))]
        {
            // Windows doesn't have SIGHUP
            Err(Error::signal_failed(-1, "SIGHUP not supported on Windows"))
        }

        Ok(())
    }
}

impl Drop for LocalProcessHandle {
    fn drop(&mut self) {
        if self.kill_on_drop {
            // Try to kill the process if it's still running
            // We use kill() instead of terminate() to ensure it dies
            // This is synchronous kill, not the async method
            let _ = self.child.kill();
        }
    }
}

impl Stream for ProcessEventStream {
    type Item = ProcessEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Send Started event first
        if !self.started_sent {
            self.started_sent = true;
            let event = ProcessEvent::new(ProcessEventType::Started { pid: self.child_id });
            return Poll::Ready(Some(event));
        }

        // Try to read from stdout
        if let Some(stdout) = &mut self.stdout {
            match Pin::new(stdout).poll_next(cx) {
                Poll::Ready(Some(Ok(line))) => {
                    // Apply filter
                    if let Some(filtered) = self.filter.filter(&line, LogSource::Stdout) {
                        let event = ProcessEvent::new_with_data(
                            ProcessEventType::Stdout,
                            filtered.to_string(),
                        );
                        return Poll::Ready(Some(event));
                    }
                    // Line was filtered out, try again
                    return self.poll_next(cx);
                }
                Poll::Ready(Some(Err(_))) => {
                    // Error reading stdout, remove it
                    self.stdout = None;
                }
                Poll::Ready(None) => {
                    // Stdout closed
                    self.stdout = None;
                }
                Poll::Pending => {}
            }
        }

        // Try to read from stderr
        if let Some(stderr) = &mut self.stderr {
            match Pin::new(stderr).poll_next(cx) {
                Poll::Ready(Some(Ok(line))) => {
                    // Apply filter
                    if let Some(filtered) = self.filter.filter(&line, LogSource::Stderr) {
                        let event = ProcessEvent::new_with_data(
                            ProcessEventType::Stderr,
                            filtered.to_string(),
                        );
                        return Poll::Ready(Some(event));
                    }
                    // Line was filtered out, try again
                    return self.poll_next(cx);
                }
                Poll::Ready(Some(Err(_))) => {
                    // Error reading stderr, remove it
                    self.stderr = None;
                }
                Poll::Ready(None) => {
                    // Stderr closed
                    self.stderr = None;
                }
                Poll::Pending => {}
            }
        }

        // If both streams are closed, the stream is exhausted
        if self.stdout.is_none() && self.stderr.is_none() {
            return Poll::Ready(None);
        }

        // One or both streams are still pending
        Poll::Pending
    }
}

// Convenience constructors
impl Command {
    /// Create a new command target
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for Command {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for ManagedProcess {
    fn default() -> Self {
        Self::new()
    }
}

// Convenience constructor for Executor with LocalLauncher
impl crate::executor::Executor<LocalLauncher> {
    /// Create an executor for local process execution
    pub fn local(service_name: impl Into<String>) -> Self {
        Self::new(service_name.into(), LocalLauncher)
    }
}

/// Attacher for connecting to existing local services
#[derive(Debug, Clone, Copy)]
pub struct LocalAttacher;

/// Handle for controlling an attached local service
pub struct LocalServiceHandle {
    service: ManagedService,
    service_name: String,
}

/// Stream of events from an attached service
pub struct AttachedEventStream {
    _service_name: String,
    log_child: Option<Child>,
    stdout: Option<Lines<BufReader<async_process::ChildStdout>>>,
    filter: Box<dyn LogFilter + Send>,
}

#[async_trait]
impl Attacher for LocalAttacher {
    type Target = ManagedService;
    type EventStream = AttachedEventStream;
    type Handle = LocalServiceHandle;

    async fn attach(
        &self,
        target: &Self::Target,
        config: AttachConfig,
    ) -> Result<(Self::EventStream, Self::Handle)> {
        // Check if service is running first
        let status = check_service_status(target).await?;
        if status != ServiceStatus::Running {
            return Err(Error::spawn_failed(format!(
                "Service '{}' is not running",
                target.name()
            )));
        }

        // Start log streaming
        let mut log_cmd = target.log_command.prepare();

        // For commands like tail, add flags before other arguments
        // Check if this is a tail-like command that supports -n and -f
        let cmd_name = target.log_command.get_program().to_string_lossy();
        let supports_tail_flags = cmd_name == "tail"
            || cmd_name.ends_with("/tail")
            || cmd_name == "journalctl"
            || cmd_name.ends_with("/journalctl");

        if supports_tail_flags {
            // Add history lines if requested
            if let Some(lines) = config.history_lines {
                if lines > 0 {
                    log_cmd.arg("-n").arg(lines.to_string());
                }
            }

            // Follow logs if requested
            if config.follow_from_start {
                log_cmd.arg("-f");
            }
        }

        log_cmd.stdout(Stdio::piped());
        log_cmd.stderr(Stdio::piped());

        let mut log_child = log_cmd
            .spawn()
            .map_err(|e| Error::spawn_failed(format!("Failed to start log streaming: {}", e)))?;

        let stdout = log_child.stdout.take().map(|s| BufReader::new(s).lines());

        let events = AttachedEventStream {
            _service_name: target.name().to_string(),
            log_child: Some(log_child),
            stdout,
            filter: Box::new(NoOpFilter),
        };

        let handle = LocalServiceHandle {
            service: target.clone(),
            service_name: target.name().to_string(),
        };

        Ok((events, handle))
    }
}

#[async_trait]
impl AttachedHandle for LocalServiceHandle {
    fn id(&self) -> String {
        self.service_name.clone()
    }

    async fn status(&self) -> Result<ServiceStatus> {
        check_service_status(&self.service).await
    }

    async fn start(&mut self) -> Result<()> {
        let mut cmd = self.service.start_command.prepare();

        let output = cmd
            .output()
            .await
            .map_err(|e| Error::spawn_failed(format!("Failed to start service: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::spawn_failed(format!(
                "Failed to start service: {}",
                stderr
            )));
        }

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        let mut cmd = self.service.stop_command.prepare();

        let output = cmd
            .output()
            .await
            .map_err(|e| Error::spawn_failed(format!("Failed to stop service: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::spawn_failed(format!(
                "Failed to stop service: {}",
                stderr
            )));
        }

        Ok(())
    }

    async fn restart(&mut self) -> Result<()> {
        if let Some(restart_cmd) = &self.service.restart_command {
            let mut cmd = restart_cmd.prepare();

            let output = cmd
                .output()
                .await
                .map_err(|e| Error::spawn_failed(format!("Failed to restart service: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(Error::spawn_failed(format!(
                    "Failed to restart service: {}",
                    stderr
                )));
            }
        } else {
            // Use stop + start
            self.stop().await?;
            self.start().await?;
        }

        Ok(())
    }

    async fn reload(&mut self) -> Result<()> {
        if let Some(reload_cmd) = &self.service.reload_command {
            let mut cmd = reload_cmd.prepare();

            let output = cmd
                .output()
                .await
                .map_err(|e| Error::spawn_failed(format!("Failed to reload service: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(Error::spawn_failed(format!(
                    "Failed to reload service: {}",
                    stderr
                )));
            }

            Ok(())
        } else {
            Err(Error::spawn_failed("Service does not support reload"))
        }
    }

    async fn disconnect(&mut self) -> Result<()> {
        // Nothing special needed for local services
        Ok(())
    }
}

impl Stream for AttachedEventStream {
    type Item = ProcessEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Read from stdout
        if let Some(stdout) = &mut self.stdout {
            match Pin::new(stdout).poll_next(cx) {
                Poll::Ready(Some(Ok(line))) => {
                    // Apply filter
                    if let Some(filtered) = self.filter.filter(&line, LogSource::Stdout) {
                        let event = ProcessEvent::new_with_data(
                            ProcessEventType::Stdout,
                            filtered.to_string(),
                        );
                        return Poll::Ready(Some(event));
                    }
                    // Line was filtered out, try again
                    return self.poll_next(cx);
                }
                Poll::Ready(Some(Err(_))) => {
                    // Error reading stdout, remove it
                    self.stdout = None;
                }
                Poll::Ready(None) => {
                    // Stdout closed
                    self.stdout = None;
                }
                Poll::Pending => return Poll::Pending,
            }
        }

        // If stdout is closed, the stream is exhausted
        if self.stdout.is_none() {
            // Clean up log child if it exists
            if let Some(mut child) = self.log_child.take() {
                let _ = child.kill();
            }
            return Poll::Ready(None);
        }

        Poll::Pending
    }
}

impl Drop for AttachedEventStream {
    fn drop(&mut self) {
        // Kill log streaming process if still running
        if let Some(mut child) = self.log_child.take() {
            let _ = child.kill();
        }
    }
}

/// Helper function to check service status
async fn check_service_status(service: &ManagedService) -> Result<ServiceStatus> {
    let mut cmd = service.status_command.prepare();

    let output = cmd
        .output()
        .await
        .map_err(|e| Error::spawn_failed(format!("Failed to check service status: {}", e)))?;

    // Most service managers return 0 for running, non-zero for not running
    if output.status.success() {
        Ok(ServiceStatus::Running)
    } else {
        // Try to determine more specific status from exit code
        match output.status.code() {
            Some(3) => Ok(ServiceStatus::Stopped), // Common for systemd
            Some(1) => Ok(ServiceStatus::Failed),  // Common for errors
            _ => Ok(ServiceStatus::Unknown),
        }
    }
}
