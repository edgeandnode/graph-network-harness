//! Runtime-agnostic command execution library
//! 
//! This crate provides a unified interface for executing commands across different
//! contexts: local processes, Docker containers, and remote SSH hosts.

#![warn(missing_docs)]

pub mod backend;
pub mod command;
pub mod error;
pub mod event;
pub mod executor;
pub mod process;
pub mod signal;
pub mod target;

pub use backend::Backend;
pub use command::Command;
pub use error::{Error, Result};
pub use event::{ServiceEvent, EventType, EventSeverity};
pub use executor::Executor;
pub use process::Process;
pub use signal::Signal;
pub use target::ExecutionTarget;

// Re-export common traits
pub use futures::stream::Stream;
pub use futures::future::Future;
