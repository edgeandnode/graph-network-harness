//! Consolidated SSH and container tests
//! 
//! IMPORTANT: All tests that use the shared SSH container MUST be modules of this single test binary.
//! This is because Rust compiles each test file into a separate binary, and static variables
//! (like our container mutex and guards) are not shared between binaries.
//! 
//! When tests using the same container are split across multiple test files, they can:
//! - Race to start/stop the container
//! - Overwhelm the SSH server with concurrent connections
//! - Cause "Connection reset by peer" errors
//! 
//! By keeping all container-using tests as modules under this single root file, we ensure:
//! - Proper mutex synchronization between tests
//! - Sequential container initialization
//! - Reliable cleanup on exit
//!
//! To add new SSH/container tests:
//! 1. Create a new module file in tests/ssh_container_tests/
//! 2. Add it as a module below
//! 
//! DO NOT create new top-level test files that use ensure_container_running()!

#![cfg(all(feature = "ssh", feature = "ssh-tests", feature = "docker-tests"))]

mod common;

// Declare all SSH/container test modules here
mod ssh_container_tests {
    pub mod shared_container;
    pub mod regression_ssh;
    pub mod cleanup_verification;
    pub mod integration_nested_ssh;
    pub mod self_hosted_ssh;
    pub mod systemd_portable_ssh;
}