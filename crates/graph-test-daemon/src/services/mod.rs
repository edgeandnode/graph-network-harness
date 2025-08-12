//! Graph Protocol services using the new Service trait architecture
//!
//! This module defines services for Graph Protocol components that implement
//! the harness-core Service trait with strongly typed actions and events.

pub mod anvil;
pub mod graph_node;
pub mod ipfs;
pub mod postgres;

// Re-export all service types
pub use anvil::{AnvilAction, AnvilEvent, AnvilService};
pub use graph_node::{GraphNodeAction, GraphNodeEvent, GraphNodeService};
pub use ipfs::{IpfsAction, IpfsEvent, IpfsService};
pub use postgres::{PostgresAction, PostgresEvent, PostgresService};

/// Stack enum containing all available services
pub enum GraphTestStack {
    /// Graph Node service instance
    GraphNode(GraphNodeService),
    /// Anvil blockchain service instance
    Anvil(AnvilService),
    /// PostgreSQL database service instance
    Postgres(PostgresService),
    /// IPFS service instance
    Ipfs(IpfsService),
}
