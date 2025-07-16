//! Self-tests for the integration test infrastructure
//! 
//! This module contains tests that verify the integration test runner itself
//! works correctly. These are not tests of the indexer agent functionality,
//! but rather tests of the testing infrastructure (container management,
//! logging, image syncing, etc.).
//! 
//! ## Why self-tests?
//! 
//! The integration test runner uses Docker-in-Docker, log streaming, and other
//! complex infrastructure. These self-tests ensure that infrastructure works
//! correctly before we rely on it for actual integration tests.
//! 
//! ## Running self-tests
//! 
//! ```bash
//! # Run all self-tests
//! cargo test --bin integration-tests self_test::
//! 
//! # Run only container tests
//! cargo test --bin integration-tests self_test::container::
//! ```

#[cfg(test)]
mod container;
#[cfg(test)]
mod image_sync;

#[cfg(test)]
pub mod helpers {
    use bollard::Docker;
    
    /// Check if Docker is available on the system
    pub async fn docker_available() -> bool {
        match Docker::connect_with_local_defaults() {
            Ok(docker) => {
                // Try to ping Docker to ensure it's responsive
                docker.ping().await.is_ok()
            }
            Err(_) => false,
        }
    }
    
    /// Helper macro to skip tests if Docker is not available
    macro_rules! require_docker {
        () => {
            if !crate::self_test::helpers::docker_available().await {
                eprintln!("Skipping test: Docker not available");
                return Ok(());
            }
        };
    }
    
    pub(crate) use require_docker;
}