//! Common test utilities for service registry integration tests

pub mod test_harness;
pub mod test_services;
pub mod websocket_client;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Test timeout for async operations
pub const TEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Shared state for integration tests
pub struct TestEnvironment {
    /// Whether Docker is available
    pub docker_available: Arc<AtomicBool>,
    /// Whether SSH is configured
    pub ssh_available: Arc<AtomicBool>,
}

impl TestEnvironment {
    /// Create new test environment and detect capabilities
    pub async fn new() -> Self {
        let docker_available = Arc::new(AtomicBool::new(false));
        let ssh_available = Arc::new(AtomicBool::new(false));

        // Check Docker availability
        if let Ok(output) = std::process::Command::new("docker")
            .arg("--version")
            .output()
        {
            if output.status.success() {
                docker_available.store(true, Ordering::Relaxed);
            }
        }

        // Check SSH availability (to localhost)
        if let Ok(output) = std::process::Command::new("ssh")
            .args([
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=5",
                "localhost",
                "echo",
                "test",
            ])
            .output()
        {
            if output.status.success() {
                ssh_available.store(true, Ordering::Relaxed);
            }
        }

        Self {
            docker_available,
            ssh_available,
        }
    }

    /// Check if Docker is available
    pub fn has_docker(&self) -> bool {
        self.docker_available.load(Ordering::Relaxed)
    }

    /// Check if SSH is available
    pub fn has_ssh(&self) -> bool {
        self.ssh_available.load(Ordering::Relaxed)
    }
}

/// Trait for test scenarios
pub trait TestScenario {
    /// Name of the test scenario
    fn name(&self) -> &str;

    /// Prerequisites for running this scenario
    fn prerequisites(&self) -> Vec<&str>;

    /// Run the test scenario
    async fn run(&self) -> anyhow::Result<()>;
}

/// Macro to create a test that requires specific features
macro_rules! integration_test {
    ($name:ident, features = [$($feature:literal),*], $body:expr) => {
        #[test]
        #[cfg(all(feature = "integration-tests", $(feature = $feature),*))]
        fn $name() {
            smol::block_on(async {
                $body
            });
        }
    };
}

pub(crate) use integration_test;
