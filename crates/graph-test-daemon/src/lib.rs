//! Graph Test Daemon
//!
//! A specialized harness daemon for Graph Protocol integration testing.
//! This daemon extends the base harness functionality with Graph-specific
//! service types and actions for automated testing workflows.

#![warn(missing_docs)]

pub mod daemon;
pub mod service_factory;
pub mod services;
pub mod task_factory;
pub mod tasks;

// Export the main types
pub use daemon::GraphTestDaemon;
pub use services::{
    AnvilAction, AnvilEvent, AnvilService, GraphNodeAction, GraphNodeEvent, GraphNodeService,
    GraphTestStack, IpfsAction, IpfsEvent, IpfsService, PostgresAction, PostgresEvent,
    PostgresService,
};
pub use tasks::{
    // State machine exports (internal use)
    GraphContractsContext,
    GraphContractsDeployTaskState,
    GraphContractsDeployTaskStateMachine,
    GraphContractsEvent,
    // Task types
    GraphContractsTask,
    SubgraphContext,
    SubgraphDeployTask,
    SubgraphDeployTaskState,
    SubgraphDeployTaskStateMachine,
    SubgraphEvent,
    TapContractsContext,
    TapContractsDeployTaskState,
    TapContractsDeployTaskStateMachine,
    TapContractsEvent,
    TapContractsTask,
};

/// Re-export core types for convenience
pub use harness_core::prelude::*;

// Tests moved to services_test.rs
