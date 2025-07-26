//! Registry backend implementations

pub mod sled;

use crate::{error::Result, models::*};
use async_trait::async_trait;
use std::collections::HashMap;

/// Trait for registry storage backends
#[async_trait]
pub trait RegistryBackend: Send + Sync {
    /// Initialize the backend
    async fn init(&self) -> Result<()>;

    /// Store a service entry
    async fn put_service(&self, entry: &ServiceEntry) -> Result<()>;

    /// Get a service by name
    async fn get_service(&self, name: &str) -> Result<Option<ServiceEntry>>;

    /// List all services
    async fn list_services(&self) -> Result<Vec<ServiceEntry>>;

    /// Remove a service
    async fn remove_service(&self, name: &str) -> Result<Option<ServiceEntry>>;

    /// Get all services as a map
    async fn get_all_services(&self) -> Result<HashMap<String, ServiceEntry>>;

    /// Store event subscription
    async fn put_subscription(&self, addr: &str, subscription: &EventSubscription) -> Result<()>;

    /// Get event subscription
    async fn get_subscription(&self, addr: &str) -> Result<Option<EventSubscription>>;

    /// Remove event subscription
    async fn remove_subscription(&self, addr: &str) -> Result<()>;

    /// List all subscriptions
    async fn list_subscriptions(&self) -> Result<HashMap<String, EventSubscription>>;
}

/// Event subscription information for persistence
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EventSubscription {
    /// Subscribed event types
    pub events: Vec<EventType>,
}
