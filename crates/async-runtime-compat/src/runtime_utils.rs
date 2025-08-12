//! Runtime-agnostic utility functions
//!
//! This module provides utility functions that abstract over different async runtimes
//! to avoid repetitive cfg-flag patterns throughout the codebase.

use std::future::Future;
use std::time::Duration;

/// Sleep for the specified duration using the current runtime
///
/// This function automatically selects the appropriate sleep implementation
/// based on the enabled runtime feature.
pub async fn sleep(duration: Duration) {
    #[cfg(feature = "smol")]
    {
        smol::Timer::after(duration).await;
    }

    #[cfg(feature = "tokio")]
    {
        tokio::time::sleep(duration).await;
    }

    #[cfg(feature = "async-std")]
    {
        async_std::task::sleep(duration).await;
    }

    #[cfg(not(any(feature = "smol", feature = "tokio", feature = "async-std")))]
    {
        compile_error!("One of the runtime features must be enabled: smol, tokio, or async-std");
    }
}

/// Timeout error returned when a future doesn't complete within the specified duration
#[derive(Debug, Clone, Copy)]
pub struct TimeoutError;

impl std::fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "operation timed out")
    }
}

impl std::error::Error for TimeoutError {}

/// Run a future with a timeout using the current runtime
///
/// Returns `Ok(T)` if the future completes within the timeout,
/// or `Err(TimeoutError)` if it times out.
pub async fn timeout<F, T>(duration: Duration, future: F) -> Result<T, TimeoutError>
where
    F: Future<Output = T>,
{
    #[cfg(feature = "smol")]
    {
        use futures::future::{select, Either};
        let timer = smol::Timer::after(duration);
        let future = Box::pin(future);

        match select(timer, future).await {
            Either::Left(_) => Err(TimeoutError),
            Either::Right((result, _)) => Ok(result),
        }
    }

    #[cfg(feature = "tokio")]
    {
        tokio::time::timeout(duration, future)
            .await
            .map_err(|_| TimeoutError)
    }

    #[cfg(feature = "async-std")]
    {
        async_std::future::timeout(duration, future)
            .await
            .map_err(|_| TimeoutError)
    }

    #[cfg(not(any(feature = "smol", feature = "tokio", feature = "async-std")))]
    {
        compile_error!("One of the runtime features must be enabled: smol, tokio, or async-std");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "smol")]
    #[test]
    fn test_sleep() {
        smol::block_on(async {
            let start = std::time::Instant::now();
            sleep(Duration::from_millis(100)).await;
            let elapsed = start.elapsed();
            assert!(elapsed >= Duration::from_millis(100));
            assert!(elapsed < Duration::from_millis(200));
        });
    }
}
