//! Process management traits and types

use async_trait::async_trait;
use crate::error::Result;

/// A handle to control a running process
#[async_trait]
pub trait ProcessHandle: Send + Sync {
    /// Get the process ID
    fn pid(&self) -> Option<u32>;
    
    /// Wait for the process to complete and return its exit status
    async fn wait(&mut self) -> Result<ExitStatus>;
    
    /// Send SIGTERM (or equivalent) for graceful shutdown
    async fn terminate(&mut self) -> Result<()>;
    
    /// Send SIGKILL (or equivalent) to forcefully stop the process
    async fn kill(&mut self) -> Result<()>;
    
    /// Send SIGINT (or equivalent) to interrupt the process
    async fn interrupt(&mut self) -> Result<()>;
    
    /// Send SIGHUP (or equivalent) to reload/reconfigure the process
    /// 
    /// Note: Not all processes handle SIGHUP. This is typically used
    /// by daemons to reload their configuration.
    async fn reload(&mut self) -> Result<()>;
}

/// Process exit status
#[derive(Debug, Clone)]
pub struct ExitStatus {
    /// Exit code if the process exited normally
    pub code: Option<i32>,
    /// Signal that terminated the process (Unix only)
    #[cfg(unix)]
    pub signal: Option<i32>,
}

impl ExitStatus {
    /// Returns true if the process exited successfully (code 0)
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
    
    /// Returns true if the process was terminated by a signal
    pub fn terminated_by_signal(&self) -> bool {
        #[cfg(unix)]
        {
            self.signal.is_some()
        }
        #[cfg(not(unix))]
        {
            false
        }
    }
}