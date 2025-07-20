//! Local process execution backend

use async_process::{Child, Stdio};
use async_trait::async_trait;
use futures::stream::Stream;
use futures_lite::io::{AsyncBufReadExt, BufReader, Lines};
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::command::Command;
use crate::error::{Error, Result};
use crate::event::{LogFilter, LogSource, NoOpFilter, ProcessEvent, ProcessEventType};
use crate::launcher::Launcher;
use crate::attacher::{AttachConfig, AttachedHandle, Attacher, ServiceStatus};
use crate::process::{ExitStatus, ProcessHandle};
use crate::target::{Target, ManagedService};

// Re-export target types for backwards compatibility
pub use crate::target::ManagedServiceBuilder;

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
    type Target = Target;
    type EventStream = ProcessEventStream;
    type Handle = LocalProcessHandle;

    async fn launch(
        &self,
        target: &Self::Target,
        command: Command,
    ) -> Result<(Self::EventStream, Self::Handle)> {
        match target {
            Target::Command | Target::ManagedProcess(_) => {
                // Prepare the command for execution
                let mut async_cmd = command.prepare();
                
                // Configure stdio for streaming
                async_cmd.stdout(Stdio::piped());
                async_cmd.stderr(Stdio::piped());

                let mut child = async_cmd
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

            Target::SystemdService(_service) => {
                // TODO: Use systemctl to manage the service
                Err(Error::spawn_failed("SystemdService not yet implemented"))
            }

            Target::SystemdPortable(portable) => {
                // For systemd-portable, the incoming command is expected to be a portablectl command
                // We simply execute it as-is, since the user knows what they want to do

                // Prepare the command for execution
                let mut async_cmd = command.prepare();

                // Configure stdio for streaming
                async_cmd.stdout(Stdio::piped());
                async_cmd.stderr(Stdio::piped());

                let mut child = async_cmd.spawn().map_err(|e| {
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

            Target::DockerContainer(container) => {
                // Build docker run command
                let mut docker_cmd = Command::new("docker");
                docker_cmd.arg("run").arg("-d"); // Detached mode

                // Add container name if specified
                if let Some(name) = container.name() {
                    docker_cmd.arg("--name").arg(name);
                }

                // Add environment variables
                for (key, value) in container.env() {
                    docker_cmd.arg("-e").arg(format!("{}={}", key, value));
                }

                // Add volume mounts
                for (host, container_path) in container.volumes() {
                    docker_cmd.arg("-v").arg(format!("{}:{}", host, container_path));
                }

                // Add working directory
                if let Some(dir) = container.working_dir() {
                    docker_cmd.arg("-w").arg(dir);
                }

                // Add the image
                docker_cmd.arg(container.image());

                // Add command arguments from the incoming command
                let cmd_program = command.get_program();
                if !cmd_program.is_empty() && cmd_program != "sh" {
                    docker_cmd.arg(cmd_program);
                    for arg in command.get_args() {
                        docker_cmd.arg(arg);
                    }
                }

                // Run docker command and get container ID
                let mut create_cmd = docker_cmd.prepare();
                create_cmd.stdout(Stdio::piped());
                create_cmd.stderr(Stdio::piped());

                let output = create_cmd
                    .output()
                    .await
                    .map_err(|e| Error::spawn_failed(format!("Failed to run docker: {}", e))
                        .with_layer_context("Docker"))?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(Error::spawn_failed(format!(
                        "Failed to create container: {}",
                        stderr
                    )).with_layer_context("Docker"));
                }

                // Extract container ID from output
                let container_id = String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .to_string();

                if container_id.is_empty() {
                    return Err(Error::spawn_failed("Failed to get container ID"));
                }

                // Start streaming logs
                let mut log_cmd = Command::new("docker")
                    .arg("logs")
                    .arg("-f")
                    .arg("--tail")
                    .arg("all")
                    .arg(&container_id)
                    .prepare();

                log_cmd.stdout(Stdio::piped());
                log_cmd.stderr(Stdio::piped());

                let mut log_child = log_cmd
                    .spawn()
                    .map_err(|e| Error::spawn_failed(format!("Failed to start log streaming: {}", e)))?;

                let child_id = log_child.id();
                let stdout = log_child.stdout.take().map(|s| BufReader::new(s).lines());
                let stderr = log_child.stderr.take().map(|s| BufReader::new(s).lines());

                let service_name = container
                    .name()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| format!("docker_{}", &container_id[..12]));

                let events = ProcessEventStream {
                    _service_name: service_name,
                    stdout,
                    stderr,
                    filter: Box::new(NoOpFilter),
                    started_sent: false,
                    child_id,
                };

                // Create a handle that manages the Docker container
                // For now, we'll track the log process
                let handle = LocalProcessHandle {
                    child: log_child,
                    kill_on_drop: container.remove_on_exit(),
                };

                Ok((events, handle))
            }

            Target::ComposeService(compose) => {
                // Build docker-compose command
                let mut compose_cmd = Command::new("docker-compose");
                
                // Add compose file
                compose_cmd.arg("-f").arg(compose.compose_file());

                // Add project name if specified
                if let Some(project) = compose.project_name() {
                    compose_cmd.arg("-p").arg(project);
                }

                // Run the service
                compose_cmd.arg("run").arg("-d");
                compose_cmd.arg(compose.service_name());

                // Add command arguments from the incoming command
                let cmd_program = command.get_program();
                if !cmd_program.is_empty() && cmd_program != "sh" {
                    compose_cmd.arg(cmd_program);
                    for arg in command.get_args() {
                        compose_cmd.arg(arg);
                    }
                }

                // Run the command and capture output
                let mut create_cmd = compose_cmd.prepare();
                create_cmd.stdout(Stdio::piped());
                create_cmd.stderr(Stdio::piped());

                let output = create_cmd
                    .output()
                    .await
                    .map_err(|e| Error::spawn_failed(format!("Failed to run docker-compose: {}", e))
                        .with_layer_context("DockerCompose"))?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(Error::spawn_failed(format!(
                        "Failed to start compose service: {}",
                        stderr
                    )).with_layer_context("DockerCompose"));
                }

                // Extract container ID from output
                let container_id = String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .to_string();

                if container_id.is_empty() {
                    return Err(Error::spawn_failed("Failed to get container ID"));
                }

                // Start streaming logs
                let mut log_cmd = Command::new("docker")
                    .arg("logs")
                    .arg("-f")
                    .arg("--tail")
                    .arg("all")
                    .arg(&container_id)
                    .prepare();

                log_cmd.stdout(Stdio::piped());
                log_cmd.stderr(Stdio::piped());

                let mut log_child = log_cmd
                    .spawn()
                    .map_err(|e| Error::spawn_failed(format!("Failed to start log streaming: {}", e)))?;

                let child_id = log_child.id();
                let stdout = log_child.stdout.take().map(|s| BufReader::new(s).lines());
                let stderr = log_child.stderr.take().map(|s| BufReader::new(s).lines());

                let events = ProcessEventStream {
                    _service_name: compose.service_name().to_string(),
                    stdout,
                    stderr,
                    filter: Box::new(NoOpFilter),
                    started_sent: false,
                    child_id,
                };

                let handle = LocalProcessHandle {
                    child: log_child,
                    kill_on_drop: false, // Don't remove compose services by default
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

