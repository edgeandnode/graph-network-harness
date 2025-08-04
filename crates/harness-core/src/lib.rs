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
pub mod task;
pub mod typed_action;

pub use error::{Error, Result};

/// Convenience prelude for harness-core users
pub mod prelude {
    pub use crate::action::{Action, ActionRegistry};
    pub use crate::client::TestClient;
    pub use crate::daemon::{BaseDaemon, Daemon};
    pub use crate::error::{Error, Result};
    pub use crate::service::{ActionDescriptor, JsonService, Service, ServiceSetup, ServiceStack, ServiceState, StatefulService};
    pub use crate::task::{DeploymentTask, JsonTask, TaskResult, TaskStack, TaskState};
    pub use crate::typed_action::TypedAction;

    // Re-export commonly used types from dependencies
    pub use async_channel::Receiver;
    pub use async_trait::async_trait;
    pub use serde::{Deserialize, Serialize};
    pub use serde_json::{Value, json};
    pub use uuid::Uuid;
}

// Re-export key types from existing crates for convenience
pub use harness_config::Config;
pub use service_orchestration::{ServiceConfig, ServiceManager, ServiceStatus};
pub use service_registry::Registry;
