//! Local process launcher implementation

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
use crate::process::{ExitStatus, ProcessHandle};
use crate::stdin::StdinHandle;
use crate::target::Target;

/// Launcher for executing processes locally
#[derive(Debug, Clone, Copy)]
pub struct LocalLauncher;

/// A handle to control a local process (launched by us)
pub struct LocalProcessHandle {
    /// The underlying child process
    child: Child,
    /// Whether to kill the process on drop
    kill_on_drop: bool,
    /// Handle for stdin (if available)
    stdin: Option<StdinHandle>,
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
        mut command: Command,
    ) -> Result<(Self::EventStream, Self::Handle)> {
        match target {
            Target::Command | Target::ManagedProcess(_) => {
                // Take stdin channel if provided
                let stdin_channel = command.take_stdin_channel();
                
                // Prepare the command for execution
                let mut async_cmd = command.prepare();

                // Configure stdio for streaming
                async_cmd.stdout(Stdio::piped());
                async_cmd.stderr(Stdio::piped());
                
                // Always configure stdin as piped so we can write to it
                async_cmd.stdin(Stdio::piped());

                let mut child = async_cmd
                    .spawn()
                    .map_err(|e| Error::spawn_failed(format!("Failed to spawn process: {}", e)))?;

                let child_id = child.id();

                let stdout = child.stdout.take().map(|s| BufReader::new(s).lines());
                let stderr = child.stderr.take().map(|s| BufReader::new(s).lines());
                let stdin = child.stdin.take();

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

                let stdin_handle = stdin.map(|s| StdinHandle::new(s, stdin_channel));

                let handle = LocalProcessHandle {
                    child,
                    kill_on_drop: true,
                    stdin: stdin_handle,
                };

                Ok((events, handle))
            }

            _ => {
                // Other target types not implemented yet
                Err(Error::spawn_failed("Target type not yet implemented for LocalLauncher"))
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

impl LocalProcessHandle {
    /// Get a mutable reference to the stdin handle
    pub fn stdin_mut(&mut self) -> Option<&mut StdinHandle> {
        self.stdin.as_mut()
    }
    
    /// Take the stdin handle, leaving None in its place
    pub fn take_stdin(&mut self) -> Option<StdinHandle> {
        self.stdin.take()
    }
    
    /// Take the stdin handle if it has a channel configured for forwarding
    pub fn take_stdin_for_forwarding(&mut self) -> Option<StdinHandle> {
        if let Some(stdin_handle) = self.stdin.as_ref() {
            if stdin_handle.has_channel() {
                return self.stdin.take();
            }
        }
        None
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

// Convenience constructor
impl crate::executor::Executor<LocalLauncher> {
    /// Create an executor for local process execution
    pub fn local(service_name: impl Into<String>) -> Self {
        Self::new(service_name.into(), LocalLauncher)
    }
}