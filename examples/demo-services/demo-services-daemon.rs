//! Demo services daemon - an example daemon that simulates a microservices platform
//!
//! Run with: cargo run --example demo-services-daemon

use anyhow::Result;
use harness_core::{daemon::Daemon, Service};
use service_orchestration::{ServiceConfig, ServiceTarget, ServiceSetup, DeploymentTask, Dependency};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

// Import service implementations
mod services;
use services::*;

#[smol::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    info!("🚀 Starting Demo Services Daemon");
    info!("📦 This simulates a social media platform's microservices");

    // Create daemon on port 8090
    let daemon = Daemon::new(8090).await?;

    // Register all demo services
    register_services(&daemon)?;
    
    info!("✅ All demo services registered!");
    info!("📋 Available services:");
    info!("   - database (PostgreSQL simulation)");
    info!("   - cache (Redis simulation)");
    info!("   - api-gateway (HTTP API Gateway)");
    info!("   - user-service (User management)");
    info!("   - feed-service (Social feed generation)");
    info!("   - notification-service (Push notifications)");
    info!("   - analytics-worker (Analytics processing)");
    info!("   - media-processor (Image/video processing)");
    info!("");
    info!("📋 Available tasks:");
    info!("   - db-migration (Database schema migration)");
    info!("   - cache-warmer (Pre-populate cache)");
    info!("   - deploy-assets (Deploy static assets)");
    info!("   - seed-data (Seed test data)");
    info!("");
    info!("🎯 Use the harness CLI to start services:");
    info!("   harness start -f examples/demo-services/configs/social-platform.yaml --all");

    // Run the daemon
    daemon.run().await?;

    Ok(())
}

fn register_services(daemon: &Daemon) -> Result<()> {
    // Database Service
    daemon.register_service_factory("demo-database", || {
        Box::new(DatabaseService::new())
    });

    // Cache Service
    daemon.register_service_factory("demo-cache", || {
        Box::new(CacheService::new())
    });

    // API Gateway
    daemon.register_service_factory("demo-api-gateway", || {
        Box::new(ApiGatewayService::new())
    });

    // User Service
    daemon.register_service_factory("demo-user-service", || {
        Box::new(UserService::new())
    });

    // Feed Service
    daemon.register_service_factory("demo-feed-service", || {
        Box::new(FeedService::new())
    });

    // Notification Service
    daemon.register_service_factory("demo-notification-service", || {
        Box::new(NotificationService::new())
    });

    // Analytics Worker
    daemon.register_service_factory("demo-analytics-worker", || {
        Box::new(AnalyticsWorker::new())
    });

    // Media Processor
    daemon.register_service_factory("demo-media-processor", || {
        Box::new(MediaProcessor::new())
    });

    // Register tasks
    daemon.register_task_factory("demo-db-migration", || {
        Box::new(DbMigrationTask::new())
    });

    daemon.register_task_factory("demo-cache-warmer", || {
        Box::new(CacheWarmerTask::new())
    });

    daemon.register_task_factory("demo-deploy-assets", || {
        Box::new(DeployAssetsTask::new())
    });

    daemon.register_task_factory("demo-seed-data", || {
        Box::new(SeedDataTask::new())
    });

    Ok(())
}

mod services {
    use super::*;

    /// Database Service - Simulates PostgreSQL
    pub struct DatabaseService;

    impl DatabaseService {
        pub fn new() -> Self {
            Self
        }
    }

    #[async_trait]
    impl Service for DatabaseService {
        fn name(&self) -> &str {
            "database"
        }

        fn service_type(&self) -> &str {
            "demo-database"
        }

        async fn config(&self) -> ServiceConfig {
            let mut env = HashMap::new();
            env.insert("DB_PORT".to_string(), "5432".to_string());

            ServiceConfig {
                name: self.name().to_string(),
                target: ServiceTarget::Process {
                    binary: "sh".to_string(),
                    args: vec!["-c".to_string(), include_str!("scripts/database.sh").to_string()],
                    env,
                    working_dir: None,
                },
                dependencies: vec![],
                health_check: Some(service_orchestration::HealthCheck {
                    command: "test".to_string(),
                    args: vec!["-f".to_string(), "/tmp/demo-db-ready".to_string()],
                    interval: 2,
                    retries: 5,
                    timeout: 5,
                }),
            }
        }
    }

    #[async_trait]
    impl ServiceSetup for DatabaseService {
        async fn is_setup_complete(&self) -> Result<bool, service_orchestration::Error> {
            Ok(std::path::Path::new("/tmp/demo-db-ready").exists())
        }

        async fn perform_setup(&self) -> Result<(), service_orchestration::Error> {
            Ok(())
        }

        async fn validate_setup(&self) -> Result<(), service_orchestration::Error> {
            Ok(())
        }
    }

    // Additional service implementations...
    pub struct CacheService;
    pub struct ApiGatewayService;
    pub struct UserService;
    pub struct FeedService;
    pub struct NotificationService;
    pub struct AnalyticsWorker;
    pub struct MediaProcessor;

    // Task implementations
    pub struct DbMigrationTask;
    pub struct CacheWarmerTask;
    pub struct DeployAssetsTask;
    pub struct SeedDataTask;

    // Implement remaining services following similar pattern...
    // For brevity, showing abbreviated implementations

    impl CacheService {
        pub fn new() -> Self { Self }
    }

    #[async_trait]
    impl Service for CacheService {
        fn name(&self) -> &str { "cache" }
        fn service_type(&self) -> &str { "demo-cache" }
        
        async fn config(&self) -> ServiceConfig {
            ServiceConfig {
                name: self.name().to_string(),
                target: ServiceTarget::Process {
                    binary: "sh".to_string(),
                    args: vec!["-c".to_string(), include_str!("scripts/cache.sh").to_string()],
                    env: HashMap::new(),
                    working_dir: None,
                },
                dependencies: vec![Dependency::Service { service: "database".to_string() }],
                health_check: Some(service_orchestration::HealthCheck {
                    command: "nc".to_string(),
                    args: vec!["-z".to_string(), "localhost".to_string(), "6379".to_string()],
                    interval: 2,
                    retries: 3,
                    timeout: 5,
                }),
            }
        }
    }

    // Similar implementations for other services...
    impl ApiGatewayService { pub fn new() -> Self { Self } }
    impl UserService { pub fn new() -> Self { Self } }
    impl FeedService { pub fn new() -> Self { Self } }
    impl NotificationService { pub fn new() -> Self { Self } }
    impl AnalyticsWorker { pub fn new() -> Self { Self } }
    impl MediaProcessor { pub fn new() -> Self { Self } }

    // Task implementations
    impl DbMigrationTask { pub fn new() -> Self { Self } }
    impl CacheWarmerTask { pub fn new() -> Self { Self } }
    impl DeployAssetsTask { pub fn new() -> Self { Self } }
    impl SeedDataTask { pub fn new() -> Self { Self } }
}