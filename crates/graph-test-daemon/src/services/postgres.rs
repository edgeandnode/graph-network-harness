//! PostgreSQL database service implementation
//!
//! This module provides the PostgreSQL database service for the Graph Protocol stack.

use async_channel::Receiver;
use async_trait::async_trait;
use harness_core::action::JsonAction;
use harness_core::config_traits::ServiceFromConfig;
use harness_core::{
    Error,
    service::{Service, ServiceEvents, ServiceSetup},
};
use harness_macros::{json_action, json_actions};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use service_orchestration::{ServiceConfig, ServiceTarget};
use std::result::Result;
use tracing::info;

/// PostgreSQL database service
#[derive(Debug)]
pub struct PostgresService {
    db_name: String,
    port: u16,
    event_tx: async_channel::Sender<PostgresEvent>,
    event_rx: async_channel::Receiver<PostgresEvent>,
}

#[json_actions]
impl PostgresService {
    /// Create a new PostgresService with specified database name and port
    pub fn new(db_name: String, port: u16) -> Self {
        let (event_tx, event_rx) = async_channel::unbounded();
        Self {
            db_name,
            port,
            event_tx,
            event_rx,
        }
    }

    /// Check database status
    #[json_action]
    pub async fn check_status(&self) -> Result<PostgresStatusResult, Error> {
        info!("Checking PostgreSQL status for database '{}'", self.db_name);

        // In a real implementation, this would query PostgreSQL
        let result = PostgresStatusResult {
            healthy: true,
            version: "15.0".to_string(),
            connections: 5,
        };

        // Emit event
        let _ = self
            .event_tx
            .send(PostgresEvent::StatusChecked {
                healthy: result.healthy,
                version: result.version.clone(),
                connections: result.connections,
            })
            .await;

        Ok(result)
    }

    /// Backup the database
    #[json_action]
    pub async fn backup(&self, backup_path: String) -> Result<BackupResult, Error> {
        info!("Backing up database '{}' to {}", self.db_name, backup_path);

        // In a real implementation, this would perform a pg_dump
        let result = BackupResult {
            path: backup_path.clone(),
            size_bytes: 1024000, // Mock size
        };

        // Emit event
        let _ = self
            .event_tx
            .send(PostgresEvent::BackupCompleted {
                path: result.path.clone(),
                size_bytes: result.size_bytes,
            })
            .await;

        Ok(result)
    }
}

impl Default for PostgresService {
    fn default() -> Self {
        Self::new("graph-node".to_string(), 5432)
    }
}

/// Result of checking PostgreSQL status
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PostgresStatusResult {
    /// Whether the database is healthy
    pub healthy: bool,
    /// Database version
    pub version: String,
    /// Number of active connections
    pub connections: u32,
}

/// Result of backup operation
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BackupResult {
    /// Path where backup was saved
    pub path: String,
    /// Size of the backup file in bytes
    pub size_bytes: u64,
}

/// Events from PostgreSQL
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event")]
pub enum PostgresEvent {
    /// Status check result
    StatusChecked {
        /// Whether the database is healthy
        healthy: bool,
        /// Database version
        version: String,
        /// Number of active connections
        connections: u32,
    },
    /// Backup completed
    BackupCompleted {
        /// Path where backup was saved
        path: String,
        /// Size of the backup file in bytes
        size_bytes: u64,
    },
    /// Action failed
    ActionFailed {
        /// Error message
        error: String,
    },
}

impl Service for PostgresService {
    fn service_type() -> &'static str {
        "postgres"
    }

    fn name(&self) -> &str {
        "postgres"
    }

    fn description(&self) -> &str {
        "PostgreSQL database service"
    }
}

#[async_trait]
impl harness_core::service::ServiceEvents for PostgresService {
    type Event = PostgresEvent;

    fn event_stream(&self) -> Receiver<Self::Event> {
        self.event_rx.clone()
    }
}

/// ServiceSetup implementation for PostgresService
///
/// PostgreSQL setup involves ensuring the database exists and has proper permissions
#[async_trait]
impl ServiceSetup for PostgresService {
    async fn validate_setup(&self) -> Result<(), Error> {
        info!(
            "Validating PostgreSQL setup for database '{}' on port {}",
            self.db_name, self.port
        );

        // TODO: Implement actual validation
        // This should check if:
        // 1. PostgreSQL is responding on the port
        // 2. The database exists
        // 3. Required users and permissions are set up
        // 4. All required tables exist
        // 5. Required extensions are installed

        // For now, return error to indicate setup is needed
        Err(Error::service_type("PostgreSQL setup not yet complete"))
    }

    async fn perform_setup(&self) -> Result<(), Error> {
        info!(
            "Performing PostgreSQL setup for database '{}'",
            self.db_name
        );

        // TODO: Implement actual setup
        // This should:
        // 1. Create the database if it doesn't exist
        // 2. Create required users
        // 3. Grant necessary permissions
        // 4. Run initial schema migrations if needed

        Ok(())
    }
}

impl ServiceFromConfig for PostgresService {
    fn from_config(config: &ServiceConfig) -> Result<Self, Error> {
        // Extract database name from params
        let db_name = config
            .target
            .get_param_str("database")
            .or_else(|| config.target.env().get("POSTGRES_DB").cloned())
            .unwrap_or_else(|| "graph-node".to_string());

        let port = config
            .target
            .get_param_u16("port")
            .or_else(|| {
                if let ServiceTarget::Docker { ports, .. } = &config.target {
                    ports.first().cloned()
                } else {
                    None
                }
            })
            .unwrap_or(5432);

        Ok(PostgresService::new(db_name, port))
    }
}
