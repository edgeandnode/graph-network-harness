//! Error types for command execution

use thiserror::Error;

/// Unified error type for command execution
#[derive(Error, Debug)]
pub enum Error {
    /// Failed to spawn a process
    #[error("failed to spawn process: {reason}")]
    SpawnFailed {
        /// The reason for the spawn failure
        reason: String,
    },

    /// Process terminated by signal
    #[error("process terminated by signal {signal}")]
    SignalTerminated {
        /// The signal number that terminated the process
        signal: i32,
    },

    /// Failed to send signal to process
    #[error("failed to send signal {signal}: {reason}")]
    SignalFailed {
        /// The signal number that failed to send
        signal: i32,
        /// The reason for the signal failure
        reason: String,
    },

    /// Command not found
    #[error("command not found: {command}")]
    CommandNotFound {
        /// The command that was not found
        command: String,
    },

    /// I/O error
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Nix error (Unix signal handling)
    #[cfg(unix)]
    #[error(transparent)]
    Nix(#[from] nix::Error),
}

impl Error {
    /// Create a spawn failed error
    pub fn spawn_failed(reason: impl Into<String>) -> Self {
        Self::SpawnFailed {
            reason: reason.into(),
        }
    }

    /// Create a signal failed error
    pub fn signal_failed(signal: i32, reason: impl Into<String>) -> Self {
        Self::SignalFailed {
            signal,
            reason: reason.into(),
        }
    }
}
