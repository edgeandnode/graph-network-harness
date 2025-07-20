//! Runtime-agnostic command execution library
//! 
//! This crate provides a unified interface for executing commands across different
//! contexts: local processes, Docker containers, and remote SSH hosts.

#![warn(missing_docs)]

pub mod launcher;
pub mod attacher;
pub mod backends;
pub mod command;
pub mod error;
pub mod event;
pub mod executor;
pub mod process;
pub mod target;

pub use launcher::Launcher;
pub use attacher::{Attacher, AttachedHandle, AttachConfig, ServiceStatus};
pub use command::Command;
pub use error::{Error, Result};
pub use event::{ProcessEvent, ProcessEventType, LogFilter, LogSource, NoOpFilter};
pub use executor::Executor;
pub use process::{ProcessHandle, ExitStatus, ExitResult};
pub use target::{Target, ManagedProcess, ManagedProcessBuilder, SystemdService, SystemdPortable, ManagedService, ManagedServiceBuilder, DockerContainer, ComposeService};
