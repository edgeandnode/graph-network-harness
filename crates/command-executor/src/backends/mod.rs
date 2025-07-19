//! Backend implementations for different execution contexts
//! 
//! This module provides built-in backends for common execution contexts.
//! Users can also implement their own backends by implementing the [`Backend`](crate::backend::Backend) trait.
//! 
//! # Example: Custom Backend
//! 
//! ```ignore
//! use command_executor::{Backend, Process, ProcessEvent, ExitStatus};
//! use async_trait::async_trait;
//! use async_process::Command;
//! 
//! struct MyCustomBackend {
//!     // backend-specific fields
//! }
//! 
//! struct MyCustomProcess {
//!     // process-specific fields
//! }
//! 
//! #[async_trait]
//! impl Backend for MyCustomBackend {
//!     type Process = MyCustomProcess;
//!     
//!     async fn spawn(&self, target: &ExecutionTarget, command: Command) -> Result<Self::Process> {
//!         // Custom implementation
//!     }
//! }
//! 
//! #[async_trait]
//! impl Process for MyCustomProcess {
//!     // Implement required methods
//! }
//! ```

pub mod local;
pub use local::LocalBackend;

#[cfg(feature = "ssh")]
pub mod ssh;
#[cfg(feature = "ssh")]
pub use ssh::SshBackend;

#[cfg(feature = "docker")]
pub mod docker;
#[cfg(feature = "docker")]
pub use docker::DockerBackend;