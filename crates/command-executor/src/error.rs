//! Error types for command execution

use thiserror::Error;

/// Unified error type for command execution
#[derive(Error, Debug)]
pub enum Error {
    /// Failed to spawn a process
    #[error("failed to spawn process: {reason}")]
    SpawnFailed { reason: String },
    
    /// Process terminated by signal
    #[error("process terminated by signal {signal}")]
    SignalTerminated { signal: i32 },
    
    /// Failed to send signal to process
    #[error("failed to send signal {signal}: {reason}")]
    SignalFailed { signal: i32, reason: String },
    
    /// Command not found
    #[error("command not found: {command}")]
    CommandNotFound { command: String },
    
    /// SSH connection failed
    #[cfg(feature = "ssh")]
    #[error("SSH connection failed to {host}: {reason}")]
    SshConnectionFailed { host: String, reason: String },
    
    /// SSH authentication failed
    #[cfg(feature = "ssh")]
    #[error("SSH authentication failed")]
    SshAuthenticationFailed,
    
    /// SSH key not found
    #[cfg(feature = "ssh")]
    #[error("SSH key not found: {path}")]
    SshKeyNotFound { path: String },
    
    /// Container not found
    #[cfg(feature = "docker")]
    #[error("container not found: {id}")]
    ContainerNotFound { id: String },
    
    /// Docker daemon not accessible
    #[cfg(feature = "docker")]
    #[error("Docker daemon not accessible")]
    DockerDaemonNotAccessible,
    
    /// Container operation failed
    #[cfg(feature = "docker")]
    #[error("container operation failed: {reason}")]
    DockerOperationFailed { reason: String },
    
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
        Self::SpawnFailed { reason: reason.into() }
    }
    
    /// Create a signal failed error
    pub fn signal_failed(signal: i32, reason: impl Into<String>) -> Self {
        Self::SignalFailed { signal, reason: reason.into() }
    }
}

/// Result type alias
pub type Result<T> = std::result::Result<T, Error>;