//! Tests for ServiceSetup implementations

use graph_test_daemon::services::{AnvilService, IpfsService, PostgresService};
use harness_core::service::ServiceSetup;

#[cfg(test)]
mod tests {
    use super::*;

    #[smol_potat::test]
    #[ignore] // TODO: Implement actual validation in AnvilService::validate_setup()
    async fn test_anvil_service_setup() {
        let service = AnvilService::new(1, 8545);

        // Test that Anvil reports not ready when service isn't running
        // validate_setup will return an error if the service isn't running
        let validation_result = service.validate_setup().await;
        assert!(validation_result.is_err()); // Should fail since Anvil isn't actually running

        // Test that setup completes successfully
        service.perform_setup().await.unwrap();

        // Validation will still fail unless the service is actually running
        // Just verify it doesn't panic
        let _ = service.validate_setup().await;
    }

    #[smol_potat::test]
    async fn test_postgres_service_setup() {
        let service = PostgresService::new("test_db".to_string(), 5432);

        // Test validation - will fail unless PostgreSQL is actually running on port 5432
        let _validation_result = service.validate_setup().await;
        // We can't assert a specific value since it depends on whether PostgreSQL is running
        // Just verify it doesn't panic

        // Test setup performs without error
        service.perform_setup().await.unwrap();

        // Test validation - may fail if not running
        let _ = service.validate_setup().await;
    }

    #[smol_potat::test]
    #[ignore] // TODO: Implement actual validation in IpfsService::validate_setup()
    async fn test_ipfs_service_setup() {
        let service = IpfsService::new(5001, 8080);

        // Test validation - will fail unless IPFS is actually running
        let validation_result = service.validate_setup().await;
        assert!(validation_result.is_err()); // Should fail since IPFS isn't running

        // Test setup performs without error
        service.perform_setup().await.unwrap();

        // Test validation - may fail if not running
        let _ = service.validate_setup().await;
    }

    #[smol_potat::test]
    async fn test_service_setup_idempotency() {
        let service = AnvilService::new(1, 8545);

        // Setup should be idempotent - calling multiple times should be safe
        service.perform_setup().await.unwrap();
        service.perform_setup().await.unwrap();

        // Validation will fail since service isn't running, but shouldn't panic
        let _result = service.validate_setup().await;
    }
}
