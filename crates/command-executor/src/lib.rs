//! Runtime-agnostic command execution library
//!
//! This crate provides a unified interface for executing commands across different
//! contexts: local processes, Docker containers, and remote SSH hosts.

#![warn(missing_docs)]

pub mod backends;
pub mod command;
pub mod error;
pub mod event;
pub mod executor;
pub mod launcher;
pub mod layered;
pub mod process;
pub mod stdin;
pub mod target;

#[cfg(test)]
mod stdin_test;

pub use command::Command;
pub use error::Error;
pub use event::{LogFilter, LogSource, NoOpFilter, ProcessEvent, ProcessEventType};
pub use executor::Executor;
pub use launcher::Launcher;
pub use layered::{
    DockerLayer, ExecutionLayer, LayeredExecutor, LocalLayer, SshLayer, WrapperLayer,
};
pub use process::{ExitResult, ExitStatus, ProcessHandle};
pub use target::{ManagedProcess, ManagedProcessBuilder, Target};
