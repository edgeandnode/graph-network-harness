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

    /// SSH connection failed
    #[cfg(feature = "ssh")]
    #[error("SSH connection failed to {host}: {reason}")]
    SshConnectionFailed {
        /// The hostname or IP address that failed to connect
        host: String,
        /// The detailed reason for the connection failure
        reason: String,
    },

    /// SSH authentication failed
    #[cfg(feature = "ssh")]
    #[error("SSH authentication failed")]
    SshAuthenticationFailed,

    /// SSH key not found
    #[cfg(feature = "ssh")]
    #[error("SSH key not found: {path}")]
    SshKeyNotFound {
        /// The path where the SSH key was expected to be found
        path: String,
    },

    /// Container not found
    #[cfg(feature = "docker")]
    #[error("container not found: {id}")]
    ContainerNotFound {
        /// The container ID or name that was not found
        id: String,
    },

    /// Docker daemon not accessible
    #[cfg(feature = "docker")]
    #[error("Docker daemon not accessible")]
    DockerDaemonNotAccessible,

    /// Container operation failed
    #[cfg(feature = "docker")]
    #[error("container operation failed: {reason}")]
    DockerOperationFailed {
        /// The detailed reason for the Docker operation failure
        reason: String,
    },

    /// I/O error
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Nix error (Unix signal handling)
    #[cfg(unix)]
    #[error(transparent)]
    Nix(#[from] nix::Error),

}

// For convenience, re-export specific error constructors
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
    
    /// Add layer context to an error message (for backwards compatibility)
    pub fn with_layer_context(self, layer: impl Into<String>) -> Self {
        match self {
            Error::SpawnFailed { reason } => Error::SpawnFailed {
                reason: format!("{} in {} layer: {}", 
                    if reason.starts_with("Failed") { "Error" } else { "Failed" },
                    layer.into(), 
                    reason
                ),
            },
            other => other,
        }
    }
}

/// Result type alias
pub type Result<T> = std::result::Result<T, Error>;
