//! Demo daemon - simulates a microservices platform for testing
//!
//! Run with: cargo run --example demo-daemon

use anyhow::Result;
use harness_core::{daemon::Daemon, Service};
use service_orchestration::{ServiceConfig, ServiceTarget, ServiceSetup, DeploymentTask, Dependency};
use async_trait::async_trait;
use std::collections::HashMap;
use tracing::info;

/// Simple demo database service
struct DemoDatabase;

#[async_trait]
impl Service for DemoDatabase {
    fn name(&self) -> &str { "database" }
    fn service_type(&self) -> &str { "demo-database" }
    
    async fn config(&self) -> ServiceConfig {
        ServiceConfig {
            name: "database".to_string(),
            target: ServiceTarget::Process {
                binary: "sh".to_string(),
                args: vec!["-c".to_string(), r#"
                    echo '[DB] Starting PostgreSQL...'
                    sleep 2
                    touch /tmp/demo-db-ready
                    echo '[DB] Database ready!'
                    while true; do
                        echo "[DB] Active connections: $((RANDOM % 50 + 10))/100"
                        sleep 3
                    done
                "#.to_string()],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![],
            health_check: Some(service_orchestration::HealthCheck {
                command: "test".to_string(),
                args: vec!["-f".to_string(), "/tmp/demo-db-ready".to_string()],
                interval: 2,
                retries: 3,
                timeout: 5,
            }),
        }
    }
}

#[async_trait]
impl ServiceSetup for DemoDatabase {
    async fn is_setup_complete(&self) -> Result<bool, service_orchestration::Error> {
        Ok(std::path::Path::new("/tmp/demo-db-ready").exists())
    }
    async fn perform_setup(&self) -> Result<(), service_orchestration::Error> { Ok(()) }
    async fn validate_setup(&self) -> Result<(), service_orchestration::Error> { Ok(()) }
}

/// Demo API Gateway service
struct DemoApiGateway;

#[async_trait]
impl Service for DemoApiGateway {
    fn name(&self) -> &str { "api-gateway" }
    fn service_type(&self) -> &str { "demo-api-gateway" }
    
    async fn config(&self) -> ServiceConfig {
        ServiceConfig {
            name: "api-gateway".to_string(),
            target: ServiceTarget::Process {
                binary: "sh".to_string(),
                args: vec!["-c".to_string(), r#"
                    echo '[API] Starting API Gateway...'
                    sleep 1
                    echo '[API] Gateway ready on port 8080'
                    while true; do
                        METHOD=$(shuf -n1 -e GET POST PUT DELETE 2>/dev/null || echo GET)
                        STATUS=$(shuf -n1 -e 200 201 404 500 2>/dev/null || echo 200)
                        echo "[API] $METHOD /api/users - Status: $STATUS"
                        sleep 1
                    done
                "#.to_string()],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![Dependency::Service { service: "database".to_string() }],
            health_check: None,
        }
    }
}

/// Demo database migration task
struct DemoMigrationTask;

#[async_trait]
impl DeploymentTask for DemoMigrationTask {
    fn name(&self) -> &str { "db-migration" }
    fn task_type(&self) -> &str { "demo-migration" }
    
    async fn config(&self) -> ServiceConfig {
        ServiceConfig {
            name: "db-migration".to_string(),
            target: ServiceTarget::Process {
                binary: "sh".to_string(),
                args: vec!["-c".to_string(), r#"
                    echo '[MIGRATION] Running database migrations...'
                    sleep 1
                    echo '[MIGRATION] Creating users table...'
                    sleep 1
                    echo '[MIGRATION] Creating posts table...'
                    sleep 1
                    echo '[MIGRATION] ✅ Migrations complete!'
                "#.to_string()],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![Dependency::Service { service: "database".to_string() }],
            health_check: None,
        }
    }
    
    async fn execute(&self, _context: &service_orchestration::OrchestrationContext) 
        -> Result<(), service_orchestration::Error> {
        Ok(())
    }
}

#[smol::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    info!("🚀 Starting Demo Daemon");
    info!("📦 This simulates a simple microservices platform");

    let daemon = Daemon::new(8090).await?;

    // Register services
    daemon.register_service_factory("demo-database", || {
        Box::new(DemoDatabase)
    });
    
    daemon.register_service_factory("demo-api-gateway", || {
        Box::new(DemoApiGateway)
    });

    // Register tasks
    daemon.register_task_factory("demo-migration", || {
        Box::new(DemoMigrationTask)
    });

    info!("✅ Demo services registered!");
    info!("");
    info!("🎯 Try these commands:");
    info!("   # Start the database");
    info!("   harness --daemon-port 8090 start database");
    info!("");
    info!("   # Start the API gateway (will start database automatically)");
    info!("   harness --daemon-port 8090 start api-gateway");
    info!("");
    info!("   # Check status");
    info!("   harness --daemon-port 8090 status");
    info!("");
    info!("   # Run migration task");
    info!("   harness --daemon-port 8090 start --task db-migration");

    daemon.run().await?;
    Ok(())
}