//! Sled database backend for service registry

use super::{EventSubscription, RegistryBackend};
use crate::{error::Result, models::*};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, error, info};

/// Sled-based registry backend
pub struct SledBackend {
    /// Database instance
    db: sled::Db,
    /// Services tree
    services: sled::Tree,
    /// Subscriptions tree
    subscriptions: sled::Tree,
}

impl SledBackend {
    /// Create a new sled backend
    pub async fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        
        // Ensure the directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        info!("Opening sled database at {:?}", path);

        // Open database
        let db = sled::open(path)?;

        // Open trees
        let services = db.open_tree("services")?;
        let subscriptions = db.open_tree("subscriptions")?;

        Ok(Self {
            db,
            services,
            subscriptions,
        })
    }

    /// Create an in-memory sled backend (for testing)
    pub async fn in_memory() -> Result<Self> {
        info!("Creating in-memory sled database");

        // Create temporary database
        let db = sled::Config::new().temporary(true).open()?;

        // Open trees
        let services = db.open_tree("services")?;
        let subscriptions = db.open_tree("subscriptions")?;

        Ok(Self {
            db,
            services,
            subscriptions,
        })
    }
}

#[async_trait]
impl RegistryBackend for SledBackend {
    async fn init(&self) -> Result<()> {
        // Flush to ensure database is ready
        self.db.flush_async().await?;
        Ok(())
    }

    async fn put_service(&self, entry: &ServiceEntry) -> Result<()> {
        debug!("Storing service: {}", entry.name);

        // Serialize entry
        let value = serde_json::to_vec(entry)?;

        // Store in database
        self.services.insert(entry.name.as_bytes(), value)?;

        // Flush to disk
        self.services.flush_async().await?;

        Ok(())
    }

    async fn get_service(&self, name: &str) -> Result<Option<ServiceEntry>> {
        debug!("Getting service: {}", name);

        match self.services.get(name.as_bytes())? {
            Some(bytes) => {
                let entry: ServiceEntry = serde_json::from_slice(&bytes)?;
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    async fn list_services(&self) -> Result<Vec<ServiceEntry>> {
        debug!("Listing all services");

        let mut services = Vec::new();

        for result in self.services.iter() {
            let (_, value) = result?;
            let entry: ServiceEntry = serde_json::from_slice(&value)?;
            services.push(entry);
        }

        Ok(services)
    }

    async fn remove_service(&self, name: &str) -> Result<Option<ServiceEntry>> {
        debug!("Removing service: {}", name);

        // Get existing entry
        let existing = self.get_service(name).await?;

        if existing.is_some() {
            // Remove from database
            self.services.remove(name.as_bytes())?;

            // Flush to disk
            self.services.flush_async().await?;
        }

        Ok(existing)
    }

    async fn get_all_services(&self) -> Result<HashMap<String, ServiceEntry>> {
        debug!("Getting all services as map");

        let mut map = HashMap::new();

        for result in self.services.iter() {
            let (key, value) = result?;
            let name = String::from_utf8_lossy(&key).into_owned();
            let entry: ServiceEntry = serde_json::from_slice(&value)?;
            map.insert(name, entry);
        }

        Ok(map)
    }

    async fn put_subscription(&self, addr: &str, subscription: &EventSubscription) -> Result<()> {
        debug!("Storing subscription for: {}", addr);

        // Serialize subscription
        let value = serde_json::to_vec(subscription)?;

        // Store in database
        self.subscriptions.insert(addr.as_bytes(), value)?;

        // Flush to disk
        self.subscriptions.flush_async().await?;

        Ok(())
    }

    async fn get_subscription(&self, addr: &str) -> Result<Option<EventSubscription>> {
        debug!("Getting subscription for: {}", addr);

        match self.subscriptions.get(addr.as_bytes())? {
            Some(bytes) => {
                let subscription: EventSubscription = serde_json::from_slice(&bytes)?;
                Ok(Some(subscription))
            }
            None => Ok(None),
        }
    }

    async fn remove_subscription(&self, addr: &str) -> Result<()> {
        debug!("Removing subscription for: {}", addr);

        self.subscriptions.remove(addr.as_bytes())?;

        // Flush to disk
        self.subscriptions.flush_async().await?;

        Ok(())
    }

    async fn list_subscriptions(&self) -> Result<HashMap<String, EventSubscription>> {
        debug!("Listing all subscriptions");

        let mut map = HashMap::new();

        for result in self.subscriptions.iter() {
            let (key, value) = result?;
            let addr = String::from_utf8_lossy(&key).into_owned();
            let subscription: EventSubscription = serde_json::from_slice(&value)?;
            map.insert(addr, subscription);
        }

        Ok(map)
    }
}

impl Drop for SledBackend {
    fn drop(&mut self) {
        // Attempt to flush on drop
        if let Err(e) = self.db.flush() {
            error!("Failed to flush database on drop: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ExecutionInfo, Location};

    #[smol_potat::test]
    async fn test_sled_backend_basic() {
        // Create in-memory backend
        let backend = SledBackend::in_memory().await.unwrap();
        backend.init().await.unwrap();

        // Create a service entry
        let service = ServiceEntry::new(
            "test-service".to_string(),
            "1.0.0".to_string(),
            ExecutionInfo::ManagedProcess {
                pid: None,
                command: "test".to_string(),
                args: vec![],
            },
            Location::Local,
        )
        .unwrap();

        // Store service
        backend.put_service(&service).await.unwrap();

        // Retrieve service
        let retrieved = backend.get_service("test-service").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "test-service");

        // List services
        let services = backend.list_services().await.unwrap();
        assert_eq!(services.len(), 1);

        // Remove service
        let removed = backend.remove_service("test-service").await.unwrap();
        assert!(removed.is_some());

        // Verify removed
        let retrieved = backend.get_service("test-service").await.unwrap();
        assert!(retrieved.is_none());
    }

    #[smol_potat::test]
    async fn test_sled_backend_subscriptions() {
        // Create in-memory backend
        let backend = SledBackend::in_memory().await.unwrap();
        backend.init().await.unwrap();

        // Create subscription
        let subscription = EventSubscription {
            events: vec![EventType::ServiceRegistered, EventType::ServiceStateChanged],
        };

        // Store subscription
        backend
            .put_subscription("127.0.0.1:8080", &subscription)
            .await
            .unwrap();

        // Retrieve subscription
        let retrieved = backend.get_subscription("127.0.0.1:8080").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().events.len(), 2);

        // List subscriptions
        let subs = backend.list_subscriptions().await.unwrap();
        assert_eq!(subs.len(), 1);

        // Remove subscription
        backend.remove_subscription("127.0.0.1:8080").await.unwrap();

        // Verify removed
        let retrieved = backend.get_subscription("127.0.0.1:8080").await.unwrap();
        assert!(retrieved.is_none());
    }

    #[smol_potat::test]
    async fn test_sled_backend_persistence() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create and populate backend
        {
            let backend = SledBackend::new(&db_path).await.unwrap();
            backend.init().await.unwrap();

            // Create services
            for i in 0..5 {
                let service = ServiceEntry::new(
                    format!("service-{}", i),
                    "1.0.0".to_string(),
                    ExecutionInfo::ManagedProcess {
                        pid: None,
                        command: format!("test-{}", i),
                        args: vec![],
                    },
                    Location::Local,
                )
                .unwrap();

                backend.put_service(&service).await.unwrap();
            }
        }

        // Reopen and verify
        {
            let backend = SledBackend::new(&db_path).await.unwrap();
            backend.init().await.unwrap();

            let services = backend.list_services().await.unwrap();
            assert_eq!(services.len(), 5);

            // Verify service names
            let names: Vec<String> = services.iter().map(|s| s.name.clone()).collect();
            assert!(names.contains(&"service-0".to_string()));
            assert!(names.contains(&"service-4".to_string()));
        }
    }
}
