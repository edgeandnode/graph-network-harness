//! Test to verify DinD container setup

use crate::network_discovery_tests::shared_dind::{check_docker, ensure_dind_container_running};

#[smol_potat::test]
async fn test_dind_container_setup() {
    if !check_docker().await {
        eprintln!("Skipping test: Docker not available");
        return;
    }

    // Just test that we can start the DinD container
    ensure_dind_container_running()
        .await
        .expect("Failed to ensure DinD container is running");
}
