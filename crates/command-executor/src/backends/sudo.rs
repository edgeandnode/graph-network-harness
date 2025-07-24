//! Sudo launcher for privilege escalation
//!
//! # Security Considerations and Limitations
//!
//! **WARNING**: This launcher has significant limitations and security implications:
//!
//! 1. **No Interactive Password Handling**: This launcher does NOT handle sudo password prompts.
//!    It assumes sudo is configured with NOPASSWD or that credentials are already cached.
//!    If sudo requires a password, commands will hang or fail.
//!
//! 2. **No stdin Support**: The command-executor library currently does not support stdin
//!    forwarding, making it impossible to interactively provide passwords.
//!
//! 3. **Privilege Escalation Visibility**: Commands executed through this launcher run with
//!    elevated privileges. This may not be immediately obvious when reading code.
//!
//! 4. **Event Stream Security**: All command output (stdout/stderr) is streamed as events.
//!    Sensitive information from privileged commands may be exposed in these streams.
//!
//! 5. **Password in Command Line**: Never pass passwords as command arguments as they
//!    may be visible in process lists and logs.
//!
//! # Recommended Usage
//!
//! - Configure sudoers with NOPASSWD for specific commands
//! - Use in controlled environments (like containers) where sudo is pre-configured
//! - Consider using SSH with a privileged user instead
//! - Always validate and sanitize inputs to prevent privilege escalation attacks
//!
//! # Example
//!
//! ```no_run
//! use command_executor::{Executor, Command, Target};
//! use command_executor::backends::{local::LocalLauncher, sudo::SudoLauncher};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a sudo wrapper around local launcher
//! let local = LocalLauncher;
//! let sudo_launcher = SudoLauncher::new(local);
//!
//! let executor = Executor::new("privileged-task".to_string(), sudo_launcher);
//!
//! // This will run: sudo systemctl restart nginx
//! let cmd = Command::builder("systemctl")
//!     .arg("restart")
//!     .arg("nginx")
//!     .build();
//!
//! let result = executor.execute(&Target::Command, cmd).await?;
//! # Ok(())
//! # }
//! ```

use crate::{error::Result, launcher::Launcher, Command};
use async_trait::async_trait;

/// Launcher that wraps another launcher to execute commands with sudo
#[derive(Debug, Clone)]
pub struct SudoLauncher<L> {
    inner: L,
}

impl<L> SudoLauncher<L> {
    /// Create a new sudo launcher wrapping the given launcher
    pub fn new(inner: L) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl<L> Launcher for SudoLauncher<L>
where
    L: Launcher + Send + Sync,
    L::Target: Send + Sync,
    L::Handle: Send + 'static,
{
    type Target = L::Target;
    type EventStream = L::EventStream;
    type Handle = L::Handle;

    async fn launch(
        &self,
        target: &Self::Target,
        command: Command,
    ) -> Result<(Self::EventStream, Self::Handle)> {
        // Build the sudo command
        let mut builder = Command::builder("sudo")
            // Preserve environment variables by default
            .arg("-E")
            // Add the original command and its arguments
            .arg(command.get_program())
            .args(command.get_args());

        // Copy environment variables
        for (key, val) in command.get_envs() {
            builder = builder.env(key, val);
        }

        // Copy working directory if set
        if let Some(dir) = command.get_current_dir() {
            builder = builder.current_dir(dir);
        }

        let sudo_command = builder.build();

        // Launch using the inner launcher
        self.inner
            .launch(target, sudo_command)
            .await
            .map_err(|e| e.with_layer_context("Sudo"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::local::LocalLauncher;

    #[test]
    fn test_sudo_launcher_creation() {
        let local = LocalLauncher;
        let sudo_launcher = SudoLauncher::new(local);

        // Just verify we can create it
        let _ = format!("{:?}", sudo_launcher);
    }
}
