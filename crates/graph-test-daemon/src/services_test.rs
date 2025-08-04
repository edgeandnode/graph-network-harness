//! Tests for the new service implementations

#[cfg(test)]
mod tests {
    use super::super::services::*;
    use async_runtime_compat;
    use harness_core::prelude::*;
    use serde_json::json;

    #[smol_potat::test]
    async fn test_graph_node_service() {
        let service = GraphNodeService::new("localhost".to_string());

        // Test service metadata
        assert_eq!(service.name(), "graph-node");
        assert!(service.description().contains("Graph Node"));

        // Test deploy subgraph action
        let action = GraphNodeAction::DeploySubgraph {
            name: "test-subgraph".to_string(),
            ipfs_hash: "QmTest123".to_string(),
            version_label: Some("v1.0.0".to_string()),
        };

        let events = service.dispatch_action(action).await.unwrap();

        // Collect events
        let mut event_count = 0;
        while let Ok(event) = events.recv().await {
            event_count += 1;
            match event {
                GraphNodeEvent::DeploymentStarted { deployment_id, .. } => {
                    assert!(deployment_id.starts_with("Qm"));
                }
                GraphNodeEvent::DeploymentProgress { percent, .. } => {
                    assert!(percent <= 100);
                }
                GraphNodeEvent::DeploymentCompleted { endpoints, .. } => {
                    assert!(!endpoints.is_empty());
                }
                _ => {}
            }
        }

        assert!(event_count > 0);
    }

    #[smol_potat::test]
    async fn test_anvil_service() {
        let service = AnvilService::new(31337, 8545);

        // Test service metadata
        assert_eq!(service.name(), "anvil");
        assert!(service.description().contains("Anvil"));

        // Test mine blocks action
        let action = AnvilAction::MineBlocks {
            count: 3,
            interval_secs: None,
        };

        let events = service.dispatch_action(action).await.unwrap();

        // Should receive 3 block mined events
        let mut block_count = 0;
        while let Ok(event) = events.recv().await {
            if let AnvilEvent::BlockMined { .. } = event {
                block_count += 1;
            }
        }

        assert_eq!(block_count, 3);
    }

    #[test]
    fn test_service_stack_registration() {
        let mut stack = ServiceStack::new();

        // Register Graph Node
        let graph_node = GraphNodeService::new("localhost".to_string());
        stack
            .register("graph-node-1".to_string(), graph_node)
            .unwrap();

        // Register Anvil
        let anvil = AnvilService::new(31337, 8545);
        stack.register("anvil-1".to_string(), anvil).unwrap();

        // Check registration
        assert_eq!(stack.list().len(), 2);
        assert!(stack.get("graph-node-1").is_some());
        assert!(stack.get("anvil-1").is_some());

        // Check all actions
        let actions = stack.all_actions();
        assert!(!actions.is_empty());
    }

    #[test]
    fn test_json_schema_generation() {
        // Test that actions and events can generate JSON schemas
        let action_schema = schemars::schema_for!(GraphNodeAction);
        let event_schema = schemars::schema_for!(GraphNodeEvent);

        // Convert to JSON to verify they're valid
        let action_json = serde_json::to_value(action_schema).unwrap();
        let event_json = serde_json::to_value(event_schema).unwrap();

        // Basic validation - schemas should have a type field
        assert!(action_json.get("$schema").is_some());
        assert!(event_json.get("$schema").is_some());
    }

    #[smol_potat::test]
    async fn test_complete_graph_stack() {
        let mut stack = ServiceStack::new();

        // Register all Graph Protocol services
        stack
            .register(
                "graph-node-1".to_string(),
                GraphNodeService::new("localhost".to_string()),
            )
            .unwrap();
        stack
            .register("anvil-1".to_string(), AnvilService::new(31337, 8545))
            .unwrap();
        stack
            .register(
                "postgres-1".to_string(),
                PostgresService::new("graph-node".to_string(), 5432),
            )
            .unwrap();
        stack
            .register("ipfs-1".to_string(), IpfsService::new(5001, 8080))
            .unwrap();

        // Check all services are registered
        assert_eq!(stack.list().len(), 4);

        // Test action discovery
        let all_actions = stack.all_actions();
        assert!(all_actions.len() >= 4); // At least one action per service

        // Test dispatching to different services
        let spawner = async_runtime_compat::smol::SmolSpawner;
        let postgres_result = stack
            .dispatch(
                "postgres-1",
                "default",
                json!({"type": "CreateDatabase", "name": "test_db"}),
                &spawner,
            )
            .await;
        assert!(postgres_result.is_ok());

        let ipfs_result = stack
            .dispatch(
                "ipfs-1",
                "default",
                json!({"type": "AddContent", "content": "Hello IPFS"}),
                &spawner,
            )
            .await;
        assert!(ipfs_result.is_ok());
    }
}
