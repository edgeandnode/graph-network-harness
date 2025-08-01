//! Runtime-agnostic command execution library
//!
//! This crate provides a unified interface for executing commands across different
//! contexts: local processes, Docker containers, and remote SSH hosts.

#![warn(missing_docs)]

pub mod attacher;
pub mod backends;
pub mod command;
pub mod error;
pub mod event;
pub mod executor;
pub mod launcher;
pub mod process;
pub mod target;

pub use attacher::{AttachConfig, AttachedHandle, Attacher, ServiceStatus};
pub use command::Command;
pub use error::{Error, Result};
pub use event::{LogFilter, LogSource, NoOpFilter, ProcessEvent, ProcessEventType};
pub use executor::Executor;
pub use launcher::Launcher;
pub use process::{ExitResult, ExitStatus, ProcessHandle};
pub use target::{
    ComposeService, DockerContainer, ManagedProcess, ManagedProcessBuilder, ManagedService,
    ManagedServiceBuilder, SystemdPortable, SystemdService, Target,
};
