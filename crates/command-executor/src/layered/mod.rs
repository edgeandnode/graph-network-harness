//! Layered execution system for runtime command composition.
//!
//! This module provides a middleware-like pattern for building execution pipelines
//! at runtime. Unlike the compile-time generic composition in the main launcher API,
//! this system allows dynamic composition of execution layers.
//!
//! # Example
//!
//! ```rust,no_run
//! use command_executor::layered::{LayeredExecutor, layers::{SshLayer, DockerLayer}};
//! use command_executor::{Command, Target, backends::local::LocalLauncher};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let executor = LayeredExecutor::new(LocalLauncher)
//!     .with_layer(SshLayer::new("user@remote-host"))
//!     .with_layer(DockerLayer::new("my-container"));
//!
//! let command = Command::new("echo").arg("hello world");
//! let target = Target::Command;
//! let (events, handle) = executor.launch(&target, command).await?;
//! # Ok(())
//! # }
//! ```

mod layers;
mod executor;
mod attacher;
#[cfg(test)]
mod integration_tests;

pub use layers::{ExecutionLayer, SshLayer, DockerLayer, LocalLayer};
pub use executor::LayeredExecutor;
pub use attacher::{
    LayeredAttacher, AttachmentLayer, 
    SshAttachmentLayer, DockerAttachmentLayer, LocalAttachmentLayer
};


/// Context passed through the execution pipeline
#[derive(Debug, Clone, Default)]
pub struct ExecutionContext {
    /// Environment variables to be set
    pub env: std::collections::HashMap<String, String>,
    /// Working directory
    pub working_dir: Option<std::path::PathBuf>,
    /// Additional metadata for layers
    pub metadata: std::collections::HashMap<String, String>,
}

impl ExecutionContext {
    /// Create a new execution context
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add an environment variable
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }
    
    /// Set the working directory
    pub fn with_working_dir(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }
    
    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}