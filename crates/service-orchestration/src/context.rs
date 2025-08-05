//! Orchestration context for runtime-agnostic service management
//!
//! This module provides a context object that carries runtime dependencies
//! like the async spawner, service registry, and configuration throughout
//! the orchestration system.

use async_runtime_compat::Spawner;
use service_registry::Registry;
use std::sync::Arc;

use crate::{StackConfig, StateManager, executors::ExecutorRegistry};

/// Context object for service orchestration
///
/// This context provides access to runtime dependencies in a runtime-agnostic way.
/// It's passed through the orchestration system to enable parallel execution,
/// service discovery, and configuration access without coupling to a specific runtime.
#[derive(Clone)]
pub struct OrchestrationContext {
    /// Runtime spawner for parallel execution
    pub spawner: Arc<dyn Spawner>,

    /// Service registry for discovery
    pub registry: Arc<Registry>,

    /// Stack configuration
    pub config: Arc<StackConfig>,

    /// Executor registry for service execution
    pub executors: Arc<ExecutorRegistry>,

    /// State manager for tracking deployment state
    pub state_manager: Arc<StateManager>,
}

impl OrchestrationContext {
    /// Create a new orchestration context
    ///
    /// The spawner is selected based on compile-time features:
    /// - `smol` feature uses SmolSpawner
    /// - `tokio` feature uses TokioSpawner
    /// - `async-std` feature uses AsyncStdSpawner
    pub fn new(config: StackConfig, registry: Registry) -> Self {
        let spawner: Arc<dyn Spawner> = {
            #[cfg(feature = "smol")]
            {
                Arc::new(async_runtime_compat::smol::SmolSpawner)
            }

            #[cfg(feature = "tokio")]
            {
                Arc::new(async_runtime_compat::tokio::TokioSpawner)
            }

            #[cfg(feature = "async-std")]
            {
                Arc::new(async_runtime_compat::async_std::AsyncStdSpawner)
            }

            #[cfg(not(any(feature = "smol", feature = "tokio", feature = "async-std")))]
            {
                compile_error!("One of the runtime features must be enabled: smol, tokio, or async-std");
            }
        };

        Self {
            spawner,
            registry: Arc::new(registry),
            config: Arc::new(config),
            executors: Arc::new(ExecutorRegistry::new()),
            state_manager: Arc::new(StateManager::new()),
        }
    }

    /// Create a context with a specific spawner
    ///
    /// This is useful for testing or when you need explicit control over the runtime.
    pub fn with_spawner(
        config: StackConfig,
        registry: Registry,
        spawner: Arc<dyn Spawner>,
    ) -> Self {
        Self {
            spawner,
            registry: Arc::new(registry),
            config: Arc::new(config),
            executors: Arc::new(ExecutorRegistry::new()),
            state_manager: Arc::new(StateManager::new()),
        }
    }

    /// Spawn a future in the background
    ///
    /// This is a convenience method that uses the context's spawner.
    pub fn spawn(
        &self,
        future: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>,
    ) {
        self.spawner.spawn(future);
    }

    /// Get a reference to the stack configuration
    pub fn config(&self) -> &StackConfig {
        &self.config
    }

    /// Get a reference to the service registry
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Get a reference to the executor registry
    pub fn executors(&self) -> &ExecutorRegistry {
        &self.executors
    }

    /// Get a reference to the state manager
    pub fn state_manager(&self) -> &StateManager {
        &self.state_manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    #[cfg(feature = "smol")]
    #[smol_potat::test]
    async fn test_context_creation() {
        // Create a test config and registry
        let config = StackConfig {
            name: "test".to_string(),
            description: Some("Test stack".to_string()),
            services: HashMap::new(),
            tasks: HashMap::new(),
        };
        let registry = Registry::new().await;

        // Create context
        let ctx = OrchestrationContext::new(config, registry);

        // Verify we can access the config
        assert_eq!(ctx.config().name, "test");
    }

    #[cfg(feature = "smol")]
    #[smol_potat::test]
    async fn test_context_spawning() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let config = StackConfig {
            name: "test".to_string(),
            description: None,
            services: HashMap::new(),
            tasks: HashMap::new(),
        };
        let registry = Registry::new().await;
        let ctx = OrchestrationContext::new(config, registry);

        // Test spawning
        let flag = Arc::new(AtomicBool::new(false));
        let flag_clone = flag.clone();

        ctx.spawn(Box::pin(async move {
            flag_clone.store(true, Ordering::SeqCst);
        }));

        // Give the spawned task time to run
        smol::Timer::after(std::time::Duration::from_millis(10)).await;

        assert!(flag.load(Ordering::SeqCst));
    }
}
