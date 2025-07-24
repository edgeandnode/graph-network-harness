//! Launcher implementations for different execution contexts
//!
//! This module provides built-in launchers for common execution contexts.
//! Users can also implement their own launchers by implementing the [`Launcher`](crate::launcher::Launcher) trait.
//!
//! # Example: Custom Launcher
//!
//! ```ignore
//! use command_executor::{Launcher, ProcessHandle, ProcessEvent, ExitStatus};
//! use async_trait::async_trait;
//! use async_process::Command;
//! use futures::stream::Stream;
//!
//! struct MyCustomLauncher {
//!     // launcher-specific fields
//! }
//!
//! struct MyCustomHandle {
//!     // handle-specific fields
//! }
//!
//! #[async_trait]
//! impl Launcher for MyCustomLauncher {
//!     type Target = MyTarget;
//!     type EventStream = MyEventStream;
//!     type Handle = MyCustomHandle;
//!     
//!     async fn launch(&self, target: &Self::Target, command: Command) -> Result<(Self::EventStream, Self::Handle)> {
//!         // Custom implementation
//!     }
//! }
//!
//! #[async_trait]
//! impl ProcessHandle for MyCustomHandle {
//!     // Implement required methods
//! }
//! ```

pub mod local;
pub use local::{LocalAttacher, LocalLauncher};

#[cfg(feature = "ssh")]
pub mod ssh;
#[cfg(feature = "ssh")]
pub use ssh::{SshAttacher, SshConfig, SshLauncher};

pub mod sudo;
pub use sudo::SudoLauncher;
