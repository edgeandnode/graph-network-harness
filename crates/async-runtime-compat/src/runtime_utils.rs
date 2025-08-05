//! Runtime-agnostic utility functions
//!
//! This module provides utility functions that abstract over different async runtimes
//! to avoid repetitive cfg-flag patterns throughout the codebase.

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