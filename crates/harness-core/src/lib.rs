//! Harness Core Library
//!
//! This crate provides the core abstractions and building blocks for creating
//! domain-specific harness daemons. It wraps and re-exports functionality from
//! the existing harness crates while providing extensibility points.

#![warn(missing_docs)]

pub mod action;
pub mod client;
pub mod daemon;
pub mod error;
pub mod service;

pub use error::{Error, Result};

/// Convenience prelude for harness-core users
pub mod prelude {
    pub use crate::action::{Action, ActionRegistry};
    pub use crate::client::TestClient;
    pub use crate::daemon::{BaseDaemon, Daemon};
    pub use crate::error::{Error, Result};
    pub use crate::service::{ServiceType, ServiceTypeRegistry};
    
    // Re-export commonly used types from dependencies
    pub use async_trait::async_trait;
    pub use serde::{Deserialize, Serialize};
    pub use serde_json::{json, Value};
    pub use uuid::Uuid;
}

// Re-export key types from existing crates for convenience  
pub use harness_config::Config;
pub use service_orchestration::{ServiceConfig, ServiceManager, ServiceStatus};
pub use service_registry::Registry;