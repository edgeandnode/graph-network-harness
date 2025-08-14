//! Harness Core Library
//!
//! This crate provides the core abstractions and building blocks for creating
//! domain-specific harness daemons. It wraps and re-exports functionality from
//! the existing harness crates while providing extensibility points.

#![warn(missing_docs)]

pub mod action;
pub mod base_service;
pub mod base_task;
pub mod client;
pub mod command_event_state;
pub mod config_traits;
pub mod daemon;
pub mod error;
pub mod service;
pub mod service_setup_task;
pub mod task;
pub mod tls;
pub mod websocket_dispatch;

pub use error::Error;

/// Convenience prelude for harness-core users
pub mod prelude {
    pub use crate::base_service::{BaseService, BaseServiceState, ServiceCommand};
    pub use crate::base_task::{BaseTask, BaseTaskState, TaskContext, TaskContextBuilder};
    pub use crate::client::TestClient;
    pub use crate::command_event_state::{
        CommandBuilder, CommandEvent, CommandExecutingTask, CommandTaskContext, CommandTaskState,
    };
    pub use crate::daemon::{BaseDaemon, Daemon};
    pub use crate::error::Error;
    pub use crate::service::{
        ActionDescriptor, JsonService, JsonServiceRegistry, Service, ServiceSetup, ServiceState,
        StatefulService,
    };
    pub use crate::service_setup_task::{
        IpfsSetupTask, PostgresSetupTask, ServiceSetupConfig, ServiceSetupTask,
    };
    pub use crate::task::{DeploymentTask, JsonTask, JsonTaskRegistry};

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
