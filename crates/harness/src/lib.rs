//! Local Network Harness for The Graph Protocol
//!
//! This crate provides a testing harness for running integration tests against
//! a local Graph network deployment. It manages Docker-in-Docker containers,
//! handles log streaming, provides service inspection capabilities, and utilities 
//! for testing Graph Protocol components.
//!
//! # Architecture
//!
//! The harness uses Docker-in-Docker (DinD) to provide isolated test environments
//! where each test run gets its own Docker daemon. This prevents container name
//! conflicts and allows parallel test execution.
//!
//! # Example
//!
//! ```no_run
//! use local_network_harness::{LocalNetworkHarness, HarnessConfig};
//! use local_network_harness::inspection::{ServiceInspector, PostgresEventHandler};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = HarnessConfig::default();
//!     let mut harness = LocalNetworkHarness::new(config)?;
//!     
//!     // Start the test environment
//!     harness.start().await?;
//!     
//!     // Set up service inspection
//!     let mut inspector = harness.create_service_inspector()?;
//!     inspector.register_handler(Box::new(PostgresEventHandler::new()));
//!     
//!     // Stream events in real-time
//!     let event_stream = inspector.event_stream();
//!     
//!     // Run your tests
//!     harness.exec(vec!["docker", "ps"], None).await?;
//!     
//!     // Cleanup happens automatically on drop
//!     Ok(())
//! }
//! ```

pub mod container;
pub mod inspection;
pub mod logging;
mod harness;

#[cfg(test)]
mod self_test;

// Re-export main types
pub use container::{ContainerConfig, DindManager};
pub use harness::{HarnessConfig, LocalNetworkHarness, TestContext};

// Re-export inspection types
pub use inspection::{ServiceInspector, ServiceEvent, EventType, EventSeverity};

// Re-export commonly used error types
pub use anyhow::{Error, Result};
