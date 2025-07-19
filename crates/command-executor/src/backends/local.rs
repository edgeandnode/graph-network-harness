//! Local process execution backend

use async_process::{Child, Command as AsyncCommand, Stdio};
use async_trait::async_trait;
use futures::stream::Stream;
use futures_lite::io::{AsyncBufReadExt, BufReader, Lines};
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::backend::Backend;
use crate::error::{Error, Result};
use crate::event::{ProcessEvent, ProcessEventType, LogSource, LogFilter, NoOpFilter};
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
    pub process_group: Option<i32>,
    /// Whether to restart on failure
    pub restart_on_failure: bool,
}

/// Execute via systemd (systemctl commands)
#[derive(Debug, Clone)]
pub struct SystemdService {
    /// The systemd unit name
    pub unit_name: String,
}

/// Execute via systemd-portable (portablectl commands)
#[derive(Debug, Clone)]
pub struct SystemdPortable {
    /// The portable service image name
    pub image_name: String,
    /// The systemd unit name
    pub unit_name: String,
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

/// Backend for executing processes locally
#[derive(Debug, Clone, Copy)]
pub struct LocalBackend;

/// A handle to control a local process
pub struct LocalProcessHandle {
    /// The underlying child process
    child: Child,
}

/// Stream of process events
pub struct ProcessEventStream {
    service_name: String,
    stdout: Option<Lines<BufReader<async_process::ChildStdout>>>,
    stderr: Option<Lines<BufReader<async_process::ChildStderr>>>,
    filter: Box<dyn LogFilter + Send>,
    started_sent: bool,
    child_id: u32,
}

#[async_trait]
impl Backend for LocalBackend {
    type Target = LocalTarget;
    type EventStream = ProcessEventStream;
    type Handle = LocalProcessHandle;

    async fn spawn(
        &self,
        target: &Self::Target,
        mut command: AsyncCommand,
    ) -> Result<(Self::EventStream, Self::Handle)> {
        match target {
            LocalTarget::Command(_) | LocalTarget::ManagedProcess(_) => {
                // Configure stdio for streaming
                command.stdout(Stdio::piped());
                command.stderr(Stdio::piped());

                let mut child = command.spawn()
                    .map_err(|e| Error::spawn_failed(format!("Failed to spawn process: {}", e)))?;

                let child_id = child.id();
                
                let stdout = child.stdout.take().map(|s| {
                    BufReader::new(s).lines()
                });
                let stderr = child.stderr.take().map(|s| {
                    BufReader::new(s).lines()
                });

                // TODO: Get service name from elsewhere (passed from Executor?)
                let service_name = "local_process".to_string();

                let events = ProcessEventStream { 
                    service_name,
                    stdout,
                    stderr,
                    filter: Box::new(NoOpFilter),
                    started_sent: false,
                    child_id,
                };

                let handle = LocalProcessHandle { child };

                Ok((events, handle))
            }

            LocalTarget::SystemdService(_service) => {
                // TODO: Use systemctl to manage the service
                Err(Error::spawn_failed("SystemdService not yet implemented"))
            }

            LocalTarget::SystemdPortable(_portable) => {
                // TODO: Use portablectl to manage the service
                Err(Error::spawn_failed("SystemdPortable not yet implemented"))
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
            self.child.kill()
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
            self.child.kill()
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
                            filtered.to_string()
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
                            filtered.to_string()
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

impl ManagedProcess {
    /// Create a new managed process target
    pub fn new() -> Self {
        Self {
            process_group: None,
            restart_on_failure: false,
        }
    }
}

impl Default for ManagedProcess {
    fn default() -> Self {
        Self::new()
    }
}

// Convenience constructor for Executor with LocalBackend
impl crate::executor::Executor<LocalBackend> {
    /// Create an executor for local process execution
    pub fn local(service_name: impl Into<String>) -> Self {
        Self::new(service_name.into(), LocalBackend)
    }
}
