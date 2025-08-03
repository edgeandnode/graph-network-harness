//! Backend implementations for different execution contexts
//!
//! This module provides the local execution backend.
//! For SSH and Docker support, use the layered execution system instead.

pub mod attacher;
pub mod launcher;

pub use attacher::{LocalAttachedHandle, LocalAttacher};
pub use launcher::{LocalLauncher, LocalProcessHandle};
