//! Registry backend implementations

pub mod memory;

use crate::{error::Error, models::*};
use async_trait::async_trait;
use std::collections::HashMap;
use std::result::Result;

/// Trait for registry storage backends
#[async_trait]
pub trait RegistryBackend: Send + Sync {
    /// Initialize the backend
    async fn init(&self) -> Result<(), Error>;

    /// Store a service entry
    async fn put_service(&self, entry: &ServiceEntry) -> Result<(), Error>;

    /// Get a service by name
    async fn get_service(&self, name: &str) -> Result<Option<ServiceEntry>, Error>;

    /// List all services
    async fn list_services(&self) -> Result<Vec<ServiceEntry>, Error>;

    /// Remove a service
    async fn remove_service(&self, name: &str) -> Result<Option<ServiceEntry>, Error>;

    /// Get all services as a map
    async fn get_all_services(&self) -> Result<HashMap<String, ServiceEntry>, Error>;

    /// Store event subscription
    async fn put_subscription(
        &self,
        addr: &str,
        subscription: &EventSubscription,
    ) -> Result<(), Error>;

    /// Get event subscription
    async fn get_subscription(&self, addr: &str) -> Result<Option<EventSubscription>, Error>;

    /// Remove event subscription
    async fn remove_subscription(&self, addr: &str) -> Result<(), Error>;

    /// List all subscriptions
    async fn list_subscriptions(&self) -> Result<HashMap<String, EventSubscription>, Error>;
}

/// Event subscription information for persistence
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EventSubscription {
    /// Subscribed event types
    pub events: Vec<EventType>,
}
