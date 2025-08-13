//! Runtime-agnostic async utilities
//!
//! This crate provides traits and implementations for spawning futures
//! across different async runtimes without coupling to a specific runtime.
//!
//! # Examples
//!
//! ```no_run
//! use async_runtime_compat::prelude::*;
//! use std::pin::Pin;
//! use std::future::Future;
//!
//! async fn example<S: Spawner>(spawner: &S) {
//!     spawner.spawn(Box::pin(async {
//!         println!("Running in the background!");
//!     }));
//! }
//!
//! // With smol
//! # #[cfg(feature = "smol")]
//! smol::block_on(async {
//!     let spawner = SmolSpawner;
//!     example(&spawner).await;
//! });
//! ```

#![warn(missing_docs)]

use cfg_if::cfg_if;
use std::future::Future;
use std::pin::Pin;

/// Trait for spawning futures on an async runtime
pub trait Spawner: Send + Sync {
    /// Spawn a future on the runtime
    ///
    /// The future will run to completion in the background.
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>);

    /// Spawn a future and detach it (alias for spawn)
    fn spawn_detached(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
        self.spawn(future);
    }
}

/// Internal spawner enum that holds the runtime-specific implementation
#[derive(Clone, Copy, Debug)]
enum InnerSpawner {
    #[cfg(feature = "tokio")]
    Tokio(crate::tokio::TokioSpawner),
    #[cfg(feature = "async-std")]
    AsyncStd(crate::async_std::AsyncStdSpawner),
    #[cfg(feature = "smol")]
    Smol(crate::smol::SmolSpawner),
}

/// A unified spawner that composes the appropriate runtime spawner based on feature flags
#[derive(Clone, Copy, Debug)]
pub struct AsyncSpawner {
    inner: InnerSpawner,
}

impl AsyncSpawner {
    /// Create a new spawner for the enabled runtime
    pub fn new() -> Self {
        cfg_if! {
            if #[cfg(feature = "tokio")] {
                AsyncSpawner {
                    inner: InnerSpawner::Tokio(crate::tokio::TokioSpawner),
                }
            } else if #[cfg(feature = "async-std")] {
                AsyncSpawner {
                    inner: InnerSpawner::AsyncStd(crate::async_std::AsyncStdSpawner),
                }
            } else if #[cfg(feature = "smol")] {
                AsyncSpawner {
                    inner: InnerSpawner::Smol(crate::smol::SmolSpawner),
                }
            } else {
                compile_error!("No async runtime feature enabled");
            }
        }
    }
}

impl Default for AsyncSpawner {
    fn default() -> Self {
        Self::new()
    }
}

impl Spawner for AsyncSpawner {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
        match self.inner {
            #[cfg(feature = "tokio")]
            InnerSpawner::Tokio(spawner) => spawner.spawn(future),
            #[cfg(feature = "async-std")]
            InnerSpawner::AsyncStd(spawner) => spawner.spawn(future),
            #[cfg(feature = "smol")]
            InnerSpawner::Smol(spawner) => spawner.spawn(future),
        }
    }
}

/// A spawner that returns a handle to the spawned task
pub trait SpawnerWithHandle: Send + Sync {
    /// The handle type returned when spawning
    type Handle: SpawnHandle;

    /// Spawn a future and return a handle to it
    fn spawn_with_handle(
        &self,
        future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
    ) -> Self::Handle;
}

/// Handle to a spawned task
pub trait SpawnHandle: Send {
    /// Detach the task, allowing it to run in the background
    fn detach(self);

    /// Abort the task if supported by the runtime
    fn abort(&self) -> Result<(), UnsupportedError> {
        Err(UnsupportedError::new("abort"))
    }

    /// Check if the task is finished
    fn is_finished(&self) -> Result<bool, UnsupportedError> {
        Err(UnsupportedError::new("is_finished"))
    }
}

/// Error returned when a runtime doesn't support an operation
#[derive(Debug)]
pub struct UnsupportedError {
    operation: &'static str,
}

impl UnsupportedError {
    fn new(operation: &'static str) -> Self {
        Self { operation }
    }
}

impl std::fmt::Display for UnsupportedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Operation '{}' not supported by this runtime",
            self.operation
        )
    }
}

impl std::error::Error for UnsupportedError {}

// Re-export spawner implementations
#[cfg(feature = "tokio")]
pub mod tokio;

#[cfg(feature = "async-std")]
pub mod async_std;

#[cfg(feature = "smol")]
pub mod smol;

pub mod runtime_utils;

/// Prelude for common imports
pub mod prelude {
    pub use crate::runtime_utils::{sleep, timeout, TimeoutError};
    pub use crate::{AsyncSpawner, SpawnHandle, Spawner, SpawnerWithHandle};

    // Runtime-specific spawners are available if needed for special cases
    #[cfg(feature = "tokio")]
    pub use crate::tokio::TokioSpawner;

    #[cfg(feature = "async-std")]
    pub use crate::async_std::AsyncStdSpawner;

    #[cfg(feature = "smol")]
    pub use crate::smol::SmolSpawner;
}

/// Create a spawner for the current runtime
///
/// This returns the unified AsyncSpawner that uses the appropriate runtime
/// based on the enabled feature flag.
pub fn current_runtime_spawner() -> AsyncSpawner {
    AsyncSpawner::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unsupported_error() {
        let err = UnsupportedError::new("test_op");
        assert_eq!(
            err.to_string(),
            "Operation 'test_op' not supported by this runtime"
        );
    }
}
