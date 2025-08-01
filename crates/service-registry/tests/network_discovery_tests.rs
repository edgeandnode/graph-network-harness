//! Consolidated network discovery tests using Docker-in-Docker
//!
//! IMPORTANT: All tests that use the shared DinD container MUST be modules of this single test binary.
//! This is because Rust compiles each test file into a separate binary, and static variables
//! (like our container mutex and guards) are not shared between binaries.
//!
//! When tests using the same container are split across multiple test files, they can:
//! - Race to start/stop the container
//! - Create conflicting Docker networks
//! - Cause cleanup issues
//!
//! By keeping all container-using tests as modules under this single root file, we ensure:
//! - Proper mutex synchronization between tests
//! - Sequential container initialization
//! - Reliable cleanup on exit
//!
//! To add new network discovery tests:
//! 1. Create a new module file in tests/network_discovery_tests/
//! 2. Add it as a module below
//!
//! DO NOT create new top-level test files that use ensure_dind_container_running()!

#![cfg(feature = "docker-tests")]

mod common;

// Declare all network discovery test modules here
mod network_discovery_tests {
    pub mod basic_tests;
    pub mod setup;
    pub mod shared_dind;
}
