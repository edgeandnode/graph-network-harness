//! Core service registry implementation

use crate::{
    error::{Error, Result},
    models::*,
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::net::SocketAddr;
use futures::lock::Mutex;
use async_fs::File;
use futures::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, info, warn};

/// Service registry
pub struct Registry {
    /// Service store
    store: Arc<Mutex<ServiceStore>>,
    /// Persistence path
    persist_path: Option<String>,
    /// Event subscribers (address -> event types)
    subscribers: Arc<Mutex<HashMap<SocketAddr, EventSubscription>>>,
}

/// Event subscription information
#[derive(Debug, Clone)]
pub struct EventSubscription {
    /// Subscribed event types
    pub events: HashSet<EventType>,
}

use std::collections::HashSet;

/// Internal service store
#[derive(Default)]
struct ServiceStore {
    /// Services by name
    services: HashMap<String, ServiceEntry>,
    /// Endpoints by service name
    endpoints_index: HashMap<String, Vec<Endpoint>>,
}

impl Registry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            store: Arc::new(Mutex::new(ServiceStore::default())),
            persist_path: None,
            subscribers: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Create a registry with persistence
    pub fn with_persistence(path: impl Into<String>) -> Self {
        Self {
            store: Arc::new(Mutex::new(ServiceStore::default())),
            persist_path: Some(path.into()),
            subscribers: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Load registry from file
    pub async fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        info!("Loading registry from {:?}", path);
        
        let mut file = File::open(path).await?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).await?;
        
        let services: HashMap<String, ServiceEntry> = serde_json::from_str(&contents)?;
        
        let mut store = ServiceStore::default();
        for (name, entry) in services {
            // Rebuild indices
            store.endpoints_index.insert(name.clone(), entry.endpoints.clone());
            store.services.insert(name, entry);
        }
        
        Ok(Self {
            store: Arc::new(Mutex::new(store)),
            persist_path: Some(path.to_string_lossy().into_owned()),
            subscribers: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    
    /// Register a new service
    pub async fn register(&self, entry: ServiceEntry) -> Result<Vec<(SocketAddr, WsMessage)>> {
        let mut store = self.store.lock().await;
        
        if store.services.contains_key(&entry.name) {
            return Err(Error::ServiceExists(entry.name.clone()));
        }
        
        info!("Registering service: {} v{}", entry.name, entry.version);
        
        // Update indices
        store.endpoints_index.insert(entry.name.clone(), entry.endpoints.clone());
        let service_name = entry.name.clone();
        store.services.insert(entry.name.clone(), entry);
        
        drop(store);
        
        // Persist if configured
        if self.persist_path.is_some() {
            self.persist().await?;
        }
        
        // Generate service registered event
        let event_data = serde_json::json!({
            "service": service_name,
            "timestamp": chrono::Utc::now(),
        });
        
        let events = self.emit_event(EventType::ServiceRegistered, event_data).await;
        
        Ok(events)
    }
    
    /// Subscribe to events
    pub async fn subscribe(&self, addr: SocketAddr, events: Vec<EventType>) -> Result<()> {
        let mut subscribers = self.subscribers.lock().await;
        let subscription = EventSubscription {
            events: events.into_iter().collect(),
        };
        subscribers.insert(addr, subscription);
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
    pub async fn emit_event(&self, event_type: EventType, data: serde_json::Value) -> Vec<(SocketAddr, WsMessage)> {
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
        let store = self.store.lock().await;
        store.services
            .get(name)
            .cloned()
            .ok_or_else(|| Error::ServiceNotFound(name.to_string()))
    }
    
    /// List all services
    pub async fn list(&self) -> Vec<ServiceEntry> {
        let store = self.store.lock().await;
        store.services.values().cloned().collect()
    }
    
    /// Update service state
    pub async fn update_state(&self, name: &str, new_state: ServiceState) -> Result<(ServiceState, Vec<(SocketAddr, WsMessage)>)> {
        let mut store = self.store.lock().await;
        
        let entry = store.services
            .get_mut(name)
            .ok_or_else(|| Error::ServiceNotFound(name.to_string()))?;
        
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
        
        drop(store);
        
        // Persist if configured
        if self.persist_path.is_some() {
            self.persist().await?;
        }
        
        // Generate state change event
        let event_data = serde_json::json!({
            "service": name,
            "old_state": old_state,
            "new_state": new_state,
            "timestamp": chrono::Utc::now(),
        });
        
        let events = self.emit_event(EventType::ServiceStateChanged, event_data).await;
        
        Ok((old_state, events))
    }
    
    /// Update service endpoints
    pub async fn update_endpoints(&self, name: &str, endpoints: Vec<Endpoint>) -> Result<Vec<(SocketAddr, WsMessage)>> {
        let mut store = self.store.lock().await;
        
        let entry = store.services
            .get_mut(name)
            .ok_or_else(|| Error::ServiceNotFound(name.to_string()))?;
        
        entry.endpoints = endpoints.clone();
        store.endpoints_index.insert(name.to_string(), endpoints.clone());
        
        drop(store);
        
        // Persist if configured
        if self.persist_path.is_some() {
            self.persist().await?;
        }
        
        // Generate endpoint updated event
        let event_data = serde_json::json!({
            "service": name,
            "endpoints": endpoints,
            "timestamp": chrono::Utc::now(),
        });
        
        let events = self.emit_event(EventType::EndpointUpdated, event_data).await;
        
        Ok(events)
    }
    
    /// Get all endpoints
    pub async fn list_endpoints(&self) -> HashMap<String, Vec<Endpoint>> {
        let store = self.store.lock().await;
        store.endpoints_index.clone()
    }
    
    /// Remove a service
    pub async fn deregister(&self, name: &str) -> Result<(ServiceEntry, Vec<(SocketAddr, WsMessage)>)> {
        let mut store = self.store.lock().await;
        
        let entry = store.services
            .remove(name)
            .ok_or_else(|| Error::ServiceNotFound(name.to_string()))?;
        
        store.endpoints_index.remove(name);
        
        drop(store);
        
        // Persist if configured
        if self.persist_path.is_some() {
            self.persist().await?;
        }
        
        // Generate service deregistered event
        let event_data = serde_json::json!({
            "service": name,
            "timestamp": chrono::Utc::now(),
        });
        
        let events = self.emit_event(EventType::ServiceDeregistered, event_data).await;
        
        Ok((entry, events))
    }
    
    /// Persist registry to disk
    pub async fn persist(&self) -> Result<()> {
        if let Some(path) = &self.persist_path {
            debug!("Persisting registry to {}", path);
            
            let store = self.store.lock().await;
            let json = serde_json::to_string_pretty(&store.services)?;
            drop(store);
            
            // Atomic write
            let temp_path = format!("{}.tmp", path);
            let mut file = File::create(&temp_path).await?;
            file.write_all(json.as_bytes()).await?;
            file.sync_all().await?;
            drop(file);
            
            async_fs::rename(&temp_path, path).await?;
        }
        
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

impl Default for Registry {
    fn default() -> Self {
        Self::new()
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
    
    #[test]
    fn test_registry_operations() {
        smol::block_on(async {
            let registry = Registry::new();
            
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
            ).unwrap();
            
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
            registry.update_state("test-service", ServiceState::Starting).await.unwrap();
            registry.update_state("test-service", ServiceState::Running).await.unwrap();
            
            // Invalid transition
            assert!(registry.update_state("test-service", ServiceState::Starting).await.is_err());
            
            // Deregister
            let (_entry, _events) = registry.deregister("test-service").await.unwrap();
            assert!(registry.get("test-service").await.is_err());
        });
    }
    
    #[test]
    fn test_event_system() {
        smol::block_on(async {
            use std::net::{IpAddr, Ipv4Addr};
            
            let registry = Registry::new();
            let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
            
            // Subscribe to events
            registry.subscribe(client_addr, vec![
                EventType::ServiceRegistered,
                EventType::ServiceStateChanged,
                EventType::ServiceDeregistered,
            ]).await.unwrap();
            
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
            ).unwrap();
            
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
            let (old_state, events) = registry.update_state("test-service", ServiceState::Starting).await.unwrap();
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
            registry.unsubscribe(client_addr, vec![EventType::ServiceRegistered]).await.unwrap();
            
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
            ).unwrap();
            
            // Register - should not get event since we unsubscribed
            let events = registry.register(service2).await.unwrap();
            assert_eq!(events.len(), 0);
        });
    }
}