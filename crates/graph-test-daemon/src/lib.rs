//! Graph Test Daemon
//!
//! A specialized harness daemon for Graph Protocol integration testing.
//! This daemon extends the base harness functionality with Graph-specific
//! service types and actions for automated testing workflows.

#![warn(missing_docs)]

pub mod actions;
pub mod daemon;
pub mod services;

pub use daemon::{GraphTestDaemon, GraphState, SubgraphInfo, BlockchainState, IndexingStatus, AllocationInfo};
pub use services::{AnvilBlockchain, IpfsNode, PostgresDb, GraphNode};

/// Re-export core types for convenience
pub use harness_core::prelude::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    #[test]
    fn test_graph_test_daemon_creation() {
        smol::block_on(async {
            let endpoint: SocketAddr = "127.0.0.1:9444".parse().unwrap();
            let daemon = GraphTestDaemon::new(endpoint).await;
            
            // Should create successfully
            assert!(daemon.is_ok());
            
            let daemon = daemon.unwrap();
            assert_eq!(daemon.endpoint(), endpoint);
            
            // Should have empty initial state
            let state = daemon.graph_state();
            assert!(state.subgraphs.is_empty());
            assert!(state.allocations.is_empty());
            assert_eq!(state.blockchain.current_block, 0);
        });
    }

    #[test]
    fn test_anvil_blockchain_service_type() {
        let anvil = AnvilBlockchain::default();
        let config = anvil.to_service_config("test-anvil".to_string()).unwrap();
        
        assert_eq!(config.name, "test-anvil");
        
        // Verify it creates a process target
        match config.target {
            service_orchestration::ServiceTarget::Process { binary, .. } => {
                assert_eq!(binary, "anvil");
            }
            _ => panic!("Expected Process target"),
        }
        
        // Should have health check
        assert!(config.health_check.is_some());
    }

    #[test]
    fn test_graph_node_service_type() {
        let graph_node = GraphNode::default();
        let config = graph_node.to_service_config("test-graph-node".to_string()).unwrap();
        
        assert_eq!(config.name, "test-graph-node");
        
        // Should have dependencies
        assert!(!config.dependencies.is_empty());
        assert!(config.dependencies.contains(&"postgres".to_string()));
        assert!(config.dependencies.contains(&"ipfs".to_string()));
        assert!(config.dependencies.contains(&"anvil".to_string()));
    }
}