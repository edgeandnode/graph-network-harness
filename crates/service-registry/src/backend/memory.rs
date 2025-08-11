//! In-memory backend for service registry

use super::{EventSubscription, RegistryBackend};
use crate::{error::Error, models::*};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;
use std::result::Result;

/// In-memory registry backend
pub struct MemoryBackend {
    /// Services storage
    services: RwLock<HashMap<String, ServiceEntry>>,
    /// Subscriptions storage
    subscriptions: RwLock<HashMap<String, EventSubscription>>,
}

impl MemoryBackend {
    /// Create a new in-memory backend
    pub fn new() -> Self {
        Self {
            services: RwLock::new(HashMap::new()),
            subscriptions: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RegistryBackend for MemoryBackend {
    async fn init(&self) -> Result<(), Error> {
        // No initialization needed for in-memory backend
        Ok(())
    }

    async fn put_service(&self, entry: &ServiceEntry) -> Result<(), Error> {
        let mut services = self.services.write().unwrap();
        services.insert(entry.name.clone(), entry.clone());
        Ok(())
    }

    async fn get_service(&self, name: &str) -> Result<Option<ServiceEntry>, Error> {
        let services = self.services.read().unwrap();
        Ok(services.get(name).cloned())
    }

    async fn list_services(&self) -> Result<Vec<ServiceEntry>, Error> {
        let services = self.services.read().unwrap();
        Ok(services.values().cloned().collect())
    }

    async fn remove_service(&self, name: &str) -> Result<Option<ServiceEntry>, Error> {
        let mut services = self.services.write().unwrap();
        Ok(services.remove(name))
    }

    async fn get_all_services(&self) -> Result<HashMap<String, ServiceEntry>, Error> {
        let services = self.services.read().unwrap();
        Ok(services.clone())
    }

    async fn put_subscription(&self, addr: &str, subscription: &EventSubscription) -> Result<(), Error> {
        let mut subscriptions = self.subscriptions.write().unwrap();
        subscriptions.insert(addr.to_string(), subscription.clone());
        Ok(())
    }

    async fn get_subscription(&self, addr: &str) -> Result<Option<EventSubscription>, Error> {
        let subscriptions = self.subscriptions.read().unwrap();
        Ok(subscriptions.get(addr).cloned())
    }

    async fn remove_subscription(&self, addr: &str) -> Result<(), Error> {
        let mut subscriptions = self.subscriptions.write().unwrap();
        subscriptions.remove(addr);
        Ok(())
    }

    async fn list_subscriptions(&self) -> Result<HashMap<String, EventSubscription>, Error> {
        let subscriptions = self.subscriptions.read().unwrap();
        Ok(subscriptions.clone())
    }
}
