//! SSH remote execution backend using CLI

use async_trait::async_trait;
use std::path::PathBuf;

use crate::attacher::{AttachConfig, AttachedHandle, Attacher, ServiceStatus};
use crate::command::Command;
use crate::error::Result;
use crate::launcher::Launcher;

/// SSH connection configuration
#[derive(Debug, Clone)]
pub struct SshConfig {
    /// Target host (hostname or IP)
    host: String,
    /// SSH user (optional, uses system default if not specified)
    user: Option<String>,
    /// SSH port (optional, defaults to 22)
    port: Option<u16>,
    /// Path to identity file (private key)
    identity_file: Option<PathBuf>,
    /// Additional SSH arguments
    extra_args: Vec<String>,
}

impl SshConfig {
    /// Create a new SSH configuration for the given host
    pub fn new(host: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            user: None,
            port: None,
            identity_file: None,
            extra_args: Vec::new(),
        }
    }

    /// Set the SSH user
    pub fn with_user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    /// Set the SSH port
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Set the identity file (private key)
    pub fn with_identity_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.identity_file = Some(path.into());
        self
    }

    /// Add extra SSH arguments
    pub fn with_extra_arg(mut self, arg: impl Into<String>) -> Self {
        self.extra_args.push(arg.into());
        self
    }

    /// Get the host string (user@host if user is specified)
    fn host_string(&self) -> String {
        if let Some(user) = &self.user {
            format!("{}@{}", user, self.host)
        } else {
            self.host.clone()
        }
    }
}

/// SSH launcher that wraps another launcher for remote execution
#[derive(Debug, Clone)]
pub struct SshLauncher<L> {
    inner: L,
    config: SshConfig,
}

impl<L> SshLauncher<L> {
    /// Create a new SSH launcher wrapping the given inner launcher
    pub fn new(inner: L, config: SshConfig) -> Self {
        Self { inner, config }
    }
}

impl SshLauncher<crate::backends::local::LocalLauncher> {
    /// Convenience constructor for SSH wrapping LocalLauncher
    pub fn to_host(host: impl Into<String>) -> Self {
        Self {
            inner: crate::backends::local::LocalLauncher,
            config: SshConfig::new(host),
        }
    }
}

#[async_trait]
impl<L> Launcher for SshLauncher<L>
where
    L: Launcher,
{
    type Target = L::Target;
    type EventStream = L::EventStream;
    type Handle = L::Handle;

    async fn launch(
        &self,
        target: &Self::Target,
        command: Command,
    ) -> Result<(Self::EventStream, Self::Handle)> {
        // Build SSH command that wraps the incoming command
        let mut ssh_cmd = Command::new("ssh");

        // Add SSH options
        if let Some(port) = self.config.port {
            ssh_cmd.arg("-p").arg(port.to_string());
        }

        if let Some(identity) = &self.config.identity_file {
            ssh_cmd
                .arg("-i")
                .arg(identity.to_string_lossy().to_string());
        }

        // Add any extra SSH arguments
        for arg in &self.config.extra_args {
            ssh_cmd.arg(arg);
        }

        // Add the host
        ssh_cmd.arg(self.config.host_string());

        // Format the remote command
        // We need to properly escape the command for the remote shell
        let remote_command = format_remote_command(&command);
        ssh_cmd.arg(remote_command);

        // Delegate to inner launcher with error context
        self.inner
            .launch(target, ssh_cmd)
            .await
            .map_err(|e| e.with_layer_context(format!("SSH[{}]", self.config.host_string())))
    }
}

/// SSH attacher that wraps another attacher for remote service attachment
#[derive(Debug, Clone)]
pub struct SshAttacher<A> {
    inner: A,
    config: SshConfig,
}

/// Wrapper for remote service handles that transforms commands through SSH
pub struct SshServiceHandle<H: AttachedHandle> {
    inner_handle: H,
    ssh_config: SshConfig,
}

#[async_trait]
impl<H: AttachedHandle> AttachedHandle for SshServiceHandle<H> {
    fn id(&self) -> String {
        format!(
            "ssh:{}/{}",
            self.ssh_config.host_string(),
            self.inner_handle.id()
        )
    }

    async fn status(&self) -> Result<ServiceStatus> {
        // Status checks are already handled by the inner handle
        // which should have SSH-wrapped commands
        self.inner_handle.status().await
    }

    async fn start(&mut self) -> Result<()> {
        self.inner_handle.start().await
    }

    async fn stop(&mut self) -> Result<()> {
        self.inner_handle.stop().await
    }

    async fn restart(&mut self) -> Result<()> {
        self.inner_handle.restart().await
    }

    async fn reload(&mut self) -> Result<()> {
        self.inner_handle.reload().await
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.inner_handle.disconnect().await
    }
}

impl<A> SshAttacher<A> {
    /// Create a new SSH attacher wrapping the given inner attacher
    pub fn new(inner: A, config: SshConfig) -> Self {
        Self { inner, config }
    }
}

impl SshAttacher<crate::backends::local::LocalAttacher> {
    /// Convenience constructor for SSH wrapping LocalAttacher
    pub fn to_host(host: impl Into<String>) -> Self {
        Self {
            inner: crate::backends::local::LocalAttacher,
            config: SshConfig::new(host),
        }
    }
}

#[async_trait]
impl<A> Attacher for SshAttacher<A>
where
    A: Attacher,
    A::Target: SshTransformable,
{
    type Target = A::Target;
    type EventStream = A::EventStream;
    type Handle = SshServiceHandle<A::Handle>;

    async fn attach(
        &self,
        target: &Self::Target,
        config: AttachConfig,
    ) -> Result<(Self::EventStream, Self::Handle)> {
        // Transform the target to wrap its commands with SSH
        let ssh_target = target.transform_for_ssh(&self.config);

        // Attach using the transformed target with error context
        let (events, inner_handle) = self
            .inner
            .attach(&ssh_target, config)
            .await
            .map_err(|e| e.with_layer_context(format!("SSH[{}]", self.config.host_string())))?;

        // Wrap the handle to ensure control commands go through SSH
        let handle = SshServiceHandle {
            inner_handle,
            ssh_config: self.config.clone(),
        };

        Ok((events, handle))
    }
}

/// Trait for targets that can be transformed for SSH execution
pub trait SshTransformable: Send + Sync {
    /// Transform this target's commands to run via SSH
    fn transform_for_ssh(&self, ssh_config: &SshConfig) -> Self;
}

/// Helper function to wrap a command with SSH
pub fn wrap_command_with_ssh(cmd: &Command, config: &SshConfig) -> Command {
    let mut ssh_cmd = Command::new("ssh");

    // Add SSH options
    if let Some(port) = config.port {
        ssh_cmd.arg("-p").arg(port.to_string());
    }

    if let Some(identity) = &config.identity_file {
        ssh_cmd
            .arg("-i")
            .arg(identity.to_string_lossy().to_string());
    }

    // Add any extra SSH arguments
    for arg in &config.extra_args {
        ssh_cmd.arg(arg);
    }

    // Add the host
    ssh_cmd.arg(config.host_string());

    // Format the remote command
    let remote_command = format_remote_command(cmd);
    ssh_cmd.arg(remote_command);

    ssh_cmd
}

/// Format a command for remote execution via SSH
fn format_remote_command(cmd: &Command) -> String {
    let program = cmd.get_program().to_string_lossy();
    let args: Vec<String> = cmd
        .get_args()
        .iter()
        .map(|arg| shell_escape(arg.to_string_lossy().to_string()))
        .collect();

    if args.is_empty() {
        program.to_string()
    } else {
        format!("{} {}", program, args.join(" "))
    }
}

/// Escape a string for safe inclusion in a shell command
fn shell_escape(s: String) -> String {
    // Simple escaping for common cases
    // A more robust implementation would handle all shell metacharacters
    if s.contains(|c: char| c.is_whitespace() || "\"'\\$`!*?<>|&;()[]{}".contains(c)) {
        // Use single quotes and escape any single quotes in the string
        format!("'{}'", s.replace('\'', "'\"'\"'"))
    } else {
        s
    }
}

// Convenience constructor for Executor with SshLauncher
impl<L> crate::executor::Executor<SshLauncher<L>>
where
    L: Launcher,
{
    /// Create an executor for SSH remote execution
    pub fn ssh(service_name: impl Into<String>, inner: L, config: SshConfig) -> Self {
        Self::new(service_name.into(), SshLauncher::new(inner, config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_escape() {
        assert_eq!(shell_escape("simple".to_string()), "simple");
        assert_eq!(shell_escape("with space".to_string()), "'with space'");
        assert_eq!(shell_escape("with'quote".to_string()), "'with'\"'\"'quote'");
        assert_eq!(shell_escape("$variable".to_string()), "'$variable'");
        assert_eq!(shell_escape("path/to/file".to_string()), "path/to/file");
    }

    #[test]
    fn test_ssh_config() {
        let config = SshConfig::new("example.com")
            .with_user("alice")
            .with_port(2222)
            .with_identity_file("/home/alice/.ssh/id_rsa");

        assert_eq!(config.host_string(), "alice@example.com");
        assert_eq!(config.port, Some(2222));
    }
}
