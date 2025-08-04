//! Test harness utilities for integration testing

use command_executor::{Command, Executor, Target, backends::LocalLauncher};
use service_registry::{Registry, ServiceEntry};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

/// Test harness for orchestrating integration tests
pub struct TestHarness {
    pub registry: Registry,
    pub executor: Executor<LocalLauncher>,
    pub temp_dir: TempDir,
}

impl TestHarness {
    /// Create a new test harness
    pub async fn new() -> anyhow::Result<Self> {
        let temp_dir = TempDir::new()?;
        let registry = Registry::with_persistence(
            temp_dir.path().join("test-registry.json").to_string_lossy(),
        )
        .await;
        let executor = Executor::new("test-harness".to_string(), LocalLauncher);

        Ok(Self {
            registry,
            executor,
            temp_dir,
        })
    }

    /// Deploy a service and manage its lifecycle
    pub async fn deploy_service(
        &self,
        service: ServiceEntry,
    ) -> anyhow::Result<ServiceDeployment<'_>> {
        let service_name = service.name.clone();

        // Register the service
        let events = self.registry.register(service).await?;

        Ok(ServiceDeployment {
            name: service_name,
            registry: Arc::new(&self.registry),
            events_received: events.len(),
        })
    }

    /// Execute a command and update service state based on result
    pub async fn execute_and_track(
        &self,
        service_name: &str,
        cmd: Command,
        target: &Target,
    ) -> anyhow::Result<()> {
        // Update to starting state
        self.registry
            .update_state(
                service_name,
                service_registry::models::ServiceState::Starting,
            )
            .await?;

        // Execute the command
        let result = self.executor.execute(target, cmd).await;

        // Update state based on result
        match &result {
            Ok(output) if output.success() => {
                self.registry
                    .update_state(
                        service_name,
                        service_registry::models::ServiceState::Running,
                    )
                    .await?;
            }
            Ok(_) | Err(_) => {
                self.registry
                    .update_state(service_name, service_registry::models::ServiceState::Failed)
                    .await?;
            }
        }

        result
            .map_err(|e| anyhow::anyhow!("Command execution failed: {}", e))
            .map(|_| ())
    }

    /// Wait for service to reach expected state
    pub async fn wait_for_service_state(
        &self,
        service_name: &str,
        expected_state: service_registry::models::ServiceState,
        timeout: Duration,
    ) -> anyhow::Result<()> {
        let start = std::time::Instant::now();

        while start.elapsed() < timeout {
            let service = self.registry.get(service_name).await?;
            if service.state == expected_state {
                return Ok(());
            }

            smol::Timer::after(Duration::from_millis(100)).await;
        }

        anyhow::bail!(
            "Service {} did not reach state {:?} within {:?}",
            service_name,
            expected_state,
            timeout
        )
    }

    /// Get temp directory path
    pub fn temp_path(&self) -> &std::path::Path {
        self.temp_dir.path()
    }
}

/// Represents a deployed service in the test harness
pub struct ServiceDeployment<'a> {
    pub name: String,
    registry: Arc<&'a Registry>,
    pub events_received: usize,
}

impl ServiceDeployment<'_> {
    /// Stop the service
    pub async fn stop(&self) -> anyhow::Result<()> {
        self.registry
            .update_state(&self.name, service_registry::models::ServiceState::Stopping)
            .await?;
        self.registry
            .update_state(&self.name, service_registry::models::ServiceState::Stopped)
            .await?;
        Ok(())
    }

    /// Remove the service
    pub async fn remove(&self) -> anyhow::Result<()> {
        self.registry.deregister(&self.name).await?;
        Ok(())
    }

    /// Get current service state
    pub async fn state(&self) -> anyhow::Result<service_registry::models::ServiceState> {
        let service = self.registry.get(&self.name).await?;
        Ok(service.state)
    }
}

/// Multi-node test environment
pub struct MultiNodeTestEnvironment {
    pub nodes: Vec<TestHarness>,
}

impl MultiNodeTestEnvironment {
    /// Create a multi-node test environment
    pub async fn new(node_count: usize) -> anyhow::Result<Self> {
        let mut nodes = Vec::new();

        for _i in 0..node_count {
            nodes.push(TestHarness::new().await?);
        }

        Ok(Self { nodes })
    }

    /// Get node by index
    pub fn node(&self, index: usize) -> Option<&TestHarness> {
        self.nodes.get(index)
    }

    /// Deploy service to specific node
    pub async fn deploy_to_node(
        &self,
        node_index: usize,
        service: ServiceEntry,
    ) -> anyhow::Result<ServiceDeployment<'_>> {
        let node = self
            .node(node_index)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_index))?;

        node.deploy_service(service).await
    }

    /// List all services across all nodes
    pub async fn list_all_services(&self) -> anyhow::Result<Vec<(usize, Vec<ServiceEntry>)>> {
        let mut all_services = Vec::new();

        for (i, node) in self.nodes.iter().enumerate() {
            let services = node.registry.list().await;
            all_services.push((i, services));
        }

        Ok(all_services)
    }
}
