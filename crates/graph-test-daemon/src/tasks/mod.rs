//! Graph Protocol deployment tasks using state machines
//!
//! This module provides robust state machine implementations for deploying
//! Graph Protocol components using the statig crate for proper state management,
//! error recovery, and progress tracking.

pub mod graph_contracts;
pub mod subgraph_deploy;
pub mod tap_contracts;

// Re-export the task types and their state machines
pub use graph_contracts::{
    GraphContractsContext, GraphContractsDeployTaskState, GraphContractsDeployTaskStateMachine,
    GraphContractsEvent, GraphContractsTask, deploy_graph_contracts,
};

pub use subgraph_deploy::{
    SubgraphContext, SubgraphDeployTask, SubgraphDeployTaskState, SubgraphDeployTaskStateMachine,
    SubgraphEvent, deploy_subgraph,
};

pub use tap_contracts::{
    TapContractsContext, TapContractsDeployTaskState, TapContractsDeployTaskStateMachine,
    TapContractsEvent, TapContractsTask, deploy_tap_contracts,
};
