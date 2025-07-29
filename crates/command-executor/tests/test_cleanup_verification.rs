//! Test to verify container cleanup on panic

#![cfg(all(feature = "ssh", feature = "ssh-tests", feature = "docker-tests"))]

mod common;

use crate::common::shared_container::ensure_container_running;

#[smol_potat::test]
async fn test_panic_cleanup() {
    // Ensure container is running
    ensure_container_running()
        .await
        .expect("Failed to ensure container is running");
    
    // Intentionally panic to test cleanup
    panic!("Testing panic cleanup!");
}