//! Tests for ServiceSetup implementations

use graph_test_daemon::services::{AnvilService, IpfsService, PostgresService};
use harness_core::service::ServiceSetup;

#[cfg(test)]
mod tests {
    use super::*;

    #[smol_potat::test]
    async fn test_anvil_service_setup() {
        let service = AnvilService::new(1, 8545);

        // Test that Anvil reports not ready when service isn't running
        let is_ready = service.is_setup_complete().await.unwrap();
        assert!(!is_ready); // Should be false since Anvil isn't actually running

        // Test that setup completes successfully
        service.perform_setup().await.unwrap();

        // Test that validation passes
        service.validate_setup().await.unwrap();
    }

    #[smol_potat::test]
    async fn test_postgres_service_setup() {
        let service = PostgresService::new("test_db".to_string(), 5432);

        // Test setup check - will be false unless PostgreSQL is actually running on port 5432
        let _is_ready = service.is_setup_complete().await.unwrap();
        // We can't assert a specific value since it depends on whether PostgreSQL is running
        // Just verify it returns without error

        // Test setup performs without error
        service.perform_setup().await.unwrap();

        // Test validation
        service.validate_setup().await.unwrap();
    }

    #[smol_potat::test]
    async fn test_ipfs_service_setup() {
        let service = IpfsService::new(5001, 8080);

        // Test setup check - will be false unless IPFS is actually running
        let is_ready = service.is_setup_complete().await.unwrap();
        assert!(!is_ready); // Should be false since IPFS isn't running

        // Test setup performs without error
        service.perform_setup().await.unwrap();

        // Test validation
        service.validate_setup().await.unwrap();
    }

    #[smol_potat::test]
    async fn test_service_setup_idempotency() {
        let service = AnvilService::new(1, 8545);

        // Setup should be idempotent - calling multiple times should be safe
        service.perform_setup().await.unwrap();
        service.perform_setup().await.unwrap();

        // Validation should still pass
        service.validate_setup().await.unwrap();
    }
}
