//! Core service registry implementation

use crate::{
    backend::RegistryBackend,
    backend::sled::SledBackend,
    error::{Error, Result},
    models::*,
};
use futures::lock::Mutex;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info};

/// Service registry with pluggable backend
pub struct Registry {
    /// Storage backend
    backend: Arc<Box<dyn RegistryBackend>>,
    /// Event subscribers (address -> event types)
    /// Note: We keep subscribers in memory for performance since they're transient
    subscribers: Arc<Mutex<HashMap<SocketAddr, EventSubscription>>>,
}

/// Event subscription information (in-memory)
#[derive(Debug, Clone)]
pub struct EventSubscription {
    /// Subscribed event types
    pub events: HashSet<EventType>,
}

use std::collections::HashSet;

impl Registry {
    /// Create a new registry with in-memory sled backend
    pub async fn new() -> Self {
        // Create in-memory backend
        let backend = SledBackend::in_memory()
            .await
            .expect("Failed to create in-memory backend");

        Self {
            backend: Arc::new(Box::new(backend)),
            subscribers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a registry with persistent sled backend
    pub async fn with_persistence(path: impl Into<String>) -> Self {
        let path = path.into();
        // Create persistent backend
        let backend = SledBackend::new(&path)
            .await
            .expect("Failed to create persistent backend");

        Self {
            backend: Arc::new(Box::new(backend)),
            subscribers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a registry with a custom backend
    pub fn with_backend(backend: Box<dyn RegistryBackend>) -> Self {
        Self {
            backend: Arc::new(backend),
            subscribers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Load registry from file (creates persistent sled backend)
    pub async fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        info!("Loading registry from {:?}", path);

        // Create backend
        let backend = SledBackend::new(path).await?;
        backend.init().await?;

        Ok(Self {
            backend: Arc::new(Box::new(backend)),
            subscribers: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Register a new service
    pub async fn register(&self, entry: ServiceEntry) -> Result<Vec<(SocketAddr, WsMessage)>> {
        // Check if service already exists
        if let Some(_) = self.backend.get_service(&entry.name).await? {
            return Err(Error::ServiceExists(entry.name.clone()));
        }

        info!("Registering service: {} v{}", entry.name, entry.version);

        // Store service
        let service_name = entry.name.clone();
        self.backend.put_service(&entry).await?;

        // Generate service registered event
        let event_data = serde_json::json!({
            "service": service_name,
            "timestamp": chrono::Utc::now(),
        });

        let events = self
            .emit_event(EventType::ServiceRegistered, event_data)
            .await;

        Ok(events)
    }

    /// Subscribe to events
    pub async fn subscribe(&self, addr: SocketAddr, events: Vec<EventType>) -> Result<()> {
        let mut subscribers = self.subscribers.lock().await;
        let subscription = EventSubscription {
            events: events.into_iter().collect(),
        };
        subscribers.insert(addr, subscription);

        // Note: We don't persist subscriptions since they're tied to active connections
        Ok(())
    }

    /// Unsubscribe from events
    pub async fn unsubscribe(&self, addr: SocketAddr, events: Vec<EventType>) -> Result<()> {
        let mut subscribers = self.subscribers.lock().await;
        if let Some(subscription) = subscribers.get_mut(&addr) {
            for event in events {
                subscription.events.remove(&event);
            }
            // Remove empty subscriptions
            if subscription.events.is_empty() {
                subscribers.remove(&addr);
            }
        }
        Ok(())
    }

    /// Remove all subscriptions for an address
    pub async fn remove_subscriber(&self, addr: SocketAddr) -> Result<()> {
        let mut subscribers = self.subscribers.lock().await;
        subscribers.remove(&addr);
        Ok(())
    }

    /// Generate events for subscribers
    pub async fn emit_event(
        &self,
        event_type: EventType,
        data: serde_json::Value,
    ) -> Vec<(SocketAddr, WsMessage)> {
        let subscribers = self.subscribers.lock().await;
        let mut events = Vec::new();

        for (addr, subscription) in subscribers.iter() {
            if subscription.events.contains(&event_type) {
                let message = WsMessage::Event {
                    event: event_type,
                    data: data.clone(),
                };
                events.push((*addr, message));
            }
        }

        events
    }

    /// Get a service by name
    pub async fn get(&self, name: &str) -> Result<ServiceEntry> {
        self.backend
            .get_service(name)
            .await?
            .ok_or_else(|| Error::ServiceNotFound(name.to_string()))
    }

    /// List all services
    pub async fn list(&self) -> Vec<ServiceEntry> {
        self.backend.list_services().await.unwrap_or_default()
    }

    /// Update service state
    pub async fn update_state(
        &self,
        name: &str,
        new_state: ServiceState,
    ) -> Result<(ServiceState, Vec<(SocketAddr, WsMessage)>)> {
        // Get current service
        let mut entry = self.get(name).await?;
        let old_state = entry.state;

        // Validate state transition
        if !Self::is_valid_transition(old_state, new_state) {
            return Err(Error::InvalidStateTransition {
                from: old_state,
                to: new_state,
            });
        }

        debug!("Service {} state: {:?} -> {:?}", name, old_state, new_state);
        entry.update_state(new_state);

        // Store updated service
        self.backend.put_service(&entry).await?;

        // Generate state change event
        let event_data = serde_json::json!({
            "service": name,
            "old_state": old_state,
            "new_state": new_state,
            "timestamp": chrono::Utc::now(),
        });

        let events = self
            .emit_event(EventType::ServiceStateChanged, event_data)
            .await;

        Ok((old_state, events))
    }

    /// Update service endpoints
    pub async fn update_endpoints(
        &self,
        name: &str,
        endpoints: Vec<Endpoint>,
    ) -> Result<Vec<(SocketAddr, WsMessage)>> {
        // Get current service
        let mut entry = self.get(name).await?;
        entry.endpoints = endpoints.clone();

        // Store updated service
        self.backend.put_service(&entry).await?;

        // Generate endpoint updated event
        let event_data = serde_json::json!({
            "service": name,
            "endpoints": endpoints,
            "timestamp": chrono::Utc::now(),
        });

        let events = self
            .emit_event(EventType::EndpointUpdated, event_data)
            .await;

        Ok(events)
    }

    /// Get all endpoints
    pub async fn list_endpoints(&self) -> HashMap<String, Vec<Endpoint>> {
        let mut endpoints = HashMap::new();

        if let Ok(services) = self.backend.list_services().await {
            for service in services {
                endpoints.insert(service.name.clone(), service.endpoints);
            }
        }

        endpoints
    }

    /// Remove a service
    pub async fn deregister(
        &self,
        name: &str,
    ) -> Result<(ServiceEntry, Vec<(SocketAddr, WsMessage)>)> {
        // Remove service
        let entry = self
            .backend
            .remove_service(name)
            .await?
            .ok_or_else(|| Error::ServiceNotFound(name.to_string()))?;

        // Generate service deregistered event
        let event_data = serde_json::json!({
            "service": name,
            "timestamp": chrono::Utc::now(),
        });

        let events = self
            .emit_event(EventType::ServiceDeregistered, event_data)
            .await;

        Ok((entry, events))
    }

    /// Add or update a service
    pub async fn add_or_update(&self, entry: ServiceEntry) -> Result<Vec<(SocketAddr, WsMessage)>> {
        info!("Adding/updating service: {} v{}", entry.name, entry.version);

        // Check if service exists
        let existing = self.backend.get_service(&entry.name).await?;
        let is_new = existing.is_none();

        // Store service
        let service_name = entry.name.clone();
        self.backend.put_service(&entry).await?;

        // Generate appropriate event
        let event_type = if is_new {
            EventType::ServiceRegistered
        } else {
            EventType::ServiceUpdated
        };

        let event_data = serde_json::json!({
            "service": service_name,
            "timestamp": chrono::Utc::now(),
        });

        let events = self.emit_event(event_type, event_data).await;

        Ok(events)
    }

    /// Persist registry to disk (no-op if using persistent backend)
    pub async fn persist(&self) -> Result<()> {
        // Backend handles its own persistence
        Ok(())
    }

    /// Check if a state transition is valid
    fn is_valid_transition(from: ServiceState, to: ServiceState) -> bool {
        use ServiceState::*;

        match (from, to) {
            // Can always transition to Failed
            (_, Failed) => true,

            // Starting transitions
            (Registered, Starting) => true,
            (Stopped, Starting) => true,
            (Failed, Starting) => true,

            // Running transitions
            (Starting, Running) => true,

            // Stopping transitions
            (Running, Stopping) => true,
            (Starting, Stopping) => true,

            // Stopped transitions
            (Stopping, Stopped) => true,

            // Invalid transitions
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_transitions() {
        use ServiceState::*;

        // Valid transitions
        assert!(Registry::is_valid_transition(Registered, Starting));
        assert!(Registry::is_valid_transition(Starting, Running));
        assert!(Registry::is_valid_transition(Running, Stopping));
        assert!(Registry::is_valid_transition(Stopping, Stopped));
        assert!(Registry::is_valid_transition(Stopped, Starting));

        // Can always fail
        assert!(Registry::is_valid_transition(Running, Failed));
        assert!(Registry::is_valid_transition(Starting, Failed));

        // Invalid transitions
        assert!(!Registry::is_valid_transition(Running, Starting));
        assert!(!Registry::is_valid_transition(Stopped, Running));
    }

    #[smol_potat::test]
    async fn test_registry_operations() {
        let registry = Registry::new().await;

        // Create a service
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

        // Register
        let _events = registry.register(service.clone()).await.unwrap();

        // Should fail on duplicate
        assert!(registry.register(service).await.is_err());

        // Get service
        let retrieved = registry.get("test-service").await.unwrap();
        assert_eq!(retrieved.name, "test-service");

        // List services
        let services = registry.list().await;
        assert_eq!(services.len(), 1);

        // Update state
        registry
            .update_state("test-service", ServiceState::Starting)
            .await
            .unwrap();
        registry
            .update_state("test-service", ServiceState::Running)
            .await
            .unwrap();

        // Invalid transition
        assert!(
            registry
                .update_state("test-service", ServiceState::Starting)
                .await
                .is_err()
        );

        // Deregister
        let (_entry, _events) = registry.deregister("test-service").await.unwrap();
        assert!(registry.get("test-service").await.is_err());
    }

    #[smol_potat::test]
    async fn test_event_system() {
        use std::net::{IpAddr, Ipv4Addr};

        let registry = Registry::new().await;
        let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        // Subscribe to events
        registry
            .subscribe(
                client_addr,
                vec![
                    EventType::ServiceRegistered,
                    EventType::ServiceStateChanged,
                    EventType::ServiceDeregistered,
                ],
            )
            .await
            .unwrap();

        // Create a service
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

        // Register and check for events
        let events = registry.register(service).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, client_addr);
        if let WsMessage::Event { event, .. } = &events[0].1 {
            assert_eq!(*event, EventType::ServiceRegistered);
        } else {
            panic!("Expected ServiceRegistered event");
        }

        // Update state and check for events
        let (old_state, events) = registry
            .update_state("test-service", ServiceState::Starting)
            .await
            .unwrap();
        assert_eq!(old_state, ServiceState::Registered);
        assert_eq!(events.len(), 1);
        if let WsMessage::Event { event, .. } = &events[0].1 {
            assert_eq!(*event, EventType::ServiceStateChanged);
        } else {
            panic!("Expected ServiceStateChanged event");
        }

        // Deregister and check for events
        let (_entry, events) = registry.deregister("test-service").await.unwrap();
        assert_eq!(events.len(), 1);
        if let WsMessage::Event { event, .. } = &events[0].1 {
            assert_eq!(*event, EventType::ServiceDeregistered);
        } else {
            panic!("Expected ServiceDeregistered event");
        }

        // Unsubscribe from some events
        registry
            .unsubscribe(client_addr, vec![EventType::ServiceRegistered])
            .await
            .unwrap();

        // Create another service
        let service2 = ServiceEntry::new(
            "test-service-2".to_string(),
            "1.0.0".to_string(),
            ExecutionInfo::ManagedProcess {
                pid: None,
                command: "test2".to_string(),
                args: vec![],
            },
            Location::Local,
        )
        .unwrap();

        // Register - should not get event since we unsubscribed
        let events = registry.register(service2).await.unwrap();
        assert_eq!(events.len(), 0);
    }

    #[smol_potat::test]
    async fn test_persistence() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let db_path = dir.path().join("registry.db");

        // Create and populate registry
        {
            let registry = Registry::load(&db_path).await.unwrap();

            // Create services
            for i in 0..3 {
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

                registry.register(service).await.unwrap();
            }
        }

        // Load and verify
        {
            let registry = Registry::load(&db_path).await.unwrap();

            let services = registry.list().await;
            assert_eq!(services.len(), 3);

            // Verify service names
            let names: Vec<String> = services.iter().map(|s| s.name.clone()).collect();
            assert!(names.contains(&"service-0".to_string()));
            assert!(names.contains(&"service-2".to_string()));
        }
    }
}
