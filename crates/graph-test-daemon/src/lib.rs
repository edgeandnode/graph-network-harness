//! Graph Test Daemon
//!
//! A specialized harness daemon for Graph Protocol integration testing.
//! This daemon extends the base harness functionality with Graph-specific
//! service types and actions for automated testing workflows.

#![warn(missing_docs)]

pub mod daemon;
pub mod services;
pub mod services_test;

// Export the main types
pub use daemon::GraphTestDaemon;
pub use services::{
    AnvilAction, AnvilEvent, AnvilService, GraphNodeAction, GraphNodeEvent, GraphNodeService,
    GraphTestStack, IpfsAction, IpfsEvent, IpfsService, PostgresAction, PostgresEvent,
    PostgresService,
};

/// Re-export core types for convenience
pub use harness_core::prelude::*;

// Tests moved to services_test.rs
