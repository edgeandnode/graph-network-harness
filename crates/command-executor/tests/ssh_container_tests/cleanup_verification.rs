//! Test to verify container cleanup on panic

use crate::common::shared_container::ensure_container_running;

/// This test intentionally panics to help debug the container cleanup mechanism.
/// When run, you should see "Panic detected, cleaning up test container..." in the output,
/// indicating that the panic handler is working correctly.
///
/// To verify cleanup actually happened, run: docker ps -a | grep command-executor-systemd-ssh-harness-test
#[smol_potat::test]
#[should_panic(expected = "Testing panic cleanup!")]
async fn test_panic_cleanup() {
    // Ensure container is running
    ensure_container_running()
        .await
        .expect("Failed to ensure container is running");

    // Intentionally panic to test cleanup
    panic!("Testing panic cleanup!");
}
