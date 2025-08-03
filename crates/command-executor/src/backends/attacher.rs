//! Local service attacher implementation

use async_process::{Child, Stdio};
use async_trait::async_trait;
use futures::stream::Stream;
use futures_lite::io::{AsyncBufReadExt, BufReader, Lines};
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::attacher::{AttachConfig, AttachedHandle, Attacher, ServiceStatus};
use crate::error::{Error, Result};
use crate::event::{LogFilter, LogSource, NoOpFilter, ProcessEvent, ProcessEventType};
use crate::target::AttachedService;

/// Attacher for connecting to existing local services
#[derive(Debug, Clone, Copy)]
pub struct LocalAttacher;

/// Handle for an attached local service
pub struct LocalAttachedHandle {
    service: AttachedService,
    service_name: String,
    log_child: Option<Child>,
}

/// Stream of events from an attached service
pub struct AttachedEventStream {
    _service_name: String,
    stdout: Option<Lines<BufReader<async_process::ChildStdout>>>,
    filter: Box<dyn LogFilter + Send>,
}

#[async_trait]
impl Attacher for LocalAttacher {
    type Target = AttachedService;
    type EventStream = AttachedEventStream;
    type Handle = LocalAttachedHandle;

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
            stdout,
            filter: Box::new(NoOpFilter),
        };

        let handle = LocalAttachedHandle {
            service: target.clone(),
            service_name: target.name().to_string(),
            log_child: Some(log_child),
        };

        Ok((events, handle))
    }
}

#[async_trait]
impl AttachedHandle for LocalAttachedHandle {
    fn id(&self) -> String {
        self.service_name.clone()
    }

    async fn status(&self) -> Result<ServiceStatus> {
        check_service_status(&self.service).await
    }

    async fn disconnect(&mut self) -> Result<()> {
        // Kill log streaming process if still running
        if let Some(mut child) = self.log_child.take() {
            let _ = child.kill();
        }
        Ok(())
    }
}

impl Drop for LocalAttachedHandle {
    fn drop(&mut self) {
        // Kill log streaming process if still running
        if let Some(mut child) = self.log_child.take() {
            let _ = child.kill();
        }
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
            return Poll::Ready(None);
        }

        Poll::Pending
    }
}

/// Helper function to check service status
async fn check_service_status(service: &AttachedService) -> Result<ServiceStatus> {
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