//! Tests for SubgraphDeployTask

use graph_test_daemon::tasks::{SubgraphDeployAction, SubgraphDeployEvent, SubgraphDeployTask};
use harness_core::task::DeploymentTask;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_task() -> (SubgraphDeployTask, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let task = SubgraphDeployTask::new(
            "http://localhost:8000".to_string(),
            "http://localhost:5001".to_string(),
            "http://localhost:8545".to_string(),
            temp_dir.path().to_string_lossy().to_string(),
        );
        (task, temp_dir)
    }

    #[smol_potat::test]
    async fn test_subgraph_deploy_task_properties() {
        let (task, _temp_dir) = create_test_task();

        assert_eq!(task.name(), "subgraph-deploy");
        assert_eq!(task.description(), "Deploy subgraphs to Graph Node");
        assert_eq!(SubgraphDeployTask::task_type(), "subgraph-deployment");
    }

    #[smol_potat::test]
    async fn test_subgraph_deploy_is_completed() {
        let (task, temp_dir) = create_test_task();

        // Initially not completed
        assert!(!task.is_completed().await.unwrap());

        // Create the expected files
        std::fs::write(temp_dir.path().join("subgraph.yaml"), "test").unwrap();
        std::fs::write(temp_dir.path().join(".deployment"), "deployed").unwrap();

        // Now should be completed
        assert!(task.is_completed().await.unwrap());
    }

    #[smol_potat::test]
    async fn test_subgraph_deploy_action() {
        let (task, _temp_dir) = create_test_task();

        let action = SubgraphDeployAction::Deploy {
            name: "test/subgraph".to_string(),
            version_label: Some("v1.0.0".to_string()),
        };

        // Execute the deploy action
        let mut event_stream = task.execute(action).await.unwrap();

        // Should receive deployment started event
        let event = event_stream.recv().await.ok();
        match event {
            Some(SubgraphDeployEvent::DeploymentStarted { name }) => {
                assert_eq!(name, "test/subgraph");
            }
            _ => panic!("Expected DeploymentStarted event"),
        }
    }

    #[smol_potat::test]
    async fn test_subgraph_build_action() {
        let (task, _temp_dir) = create_test_task();

        let action = SubgraphDeployAction::Build;

        // Execute the build action
        let mut event_stream = task.execute(action).await.unwrap();

        // Should receive build started event
        let event = event_stream.recv().await.ok();
        match event {
            Some(SubgraphDeployEvent::BuildStarted) => {
                // Expected
            }
            _ => panic!("Expected BuildStarted event"),
        }

        // Should receive error since it's not fully implemented
        let event = event_stream.recv().await.ok();
        match event {
            Some(SubgraphDeployEvent::Error { message }) => {
                assert!(message.contains("not fully implemented"));
            }
            _ => panic!("Expected Error event"),
        }
    }

    #[smol_potat::test]
    async fn test_subgraph_create_action() {
        let (task, _temp_dir) = create_test_task();

        let action = SubgraphDeployAction::Create {
            template: "scaffold-eth".to_string(),
        };

        // Execute the create action
        let mut event_stream = task.execute(action).await.unwrap();

        // Should receive error since it's not implemented
        let event = event_stream.recv().await.ok();
        match event {
            Some(SubgraphDeployEvent::Error { message }) => {
                assert!(message.contains("not implemented"));
            }
            _ => panic!("Expected Error event"),
        }
    }
}
