//! Runtime-agnostic command execution library
//! 
//! This crate provides a unified interface for executing commands across different
//! contexts: local processes, Docker containers, and remote SSH hosts.

#![warn(missing_docs)]

pub mod launcher;
pub mod backends;
pub mod error;
pub mod event;
pub mod executor;
pub mod process;
pub mod target;

pub use launcher::{Launcher, Attacher, AttachedHandle, AttachConfig, ServiceStatus};
pub use error::{Error, Result};
pub use event::{ProcessEvent, ProcessEventType, LogFilter, LogSource, NoOpFilter};
pub use executor::Executor;
pub use process::{ProcessHandle, ExitStatus};
pub use target::ExecutionTarget;
