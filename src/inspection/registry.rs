//! Registry for managing service event handlers

use std::collections::HashMap;
use tracing::debug;

use super::events::{ServiceEvent, ContainerEvent};
use super::handlers::{ServiceEventHandler, GenericEventHandler};

/// Registry that manages service event handlers and routes events to appropriate handlers
pub struct ServiceEventRegistry {
    /// Registered handlers by service name
    handlers: HashMap<String, Box<dyn ServiceEventHandler>>,
    /// Fallback handler for services without specific handlers
    fallback_handler: GenericEventHandler,
}

impl ServiceEventRegistry {
    /// Create a new service event registry
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            fallback_handler: GenericEventHandler::new("unknown".to_string()),
        }
    }

    /// Register a service event handler
    pub fn register_handler(&mut self, handler: Box<dyn ServiceEventHandler>) {
        let service_name = handler.service_name().to_string();
        debug!("Registering event handler for service: {}", service_name);
        self.handlers.insert(service_name, handler);
    }

    /// Register multiple handlers at once
    pub fn register_handlers(&mut self, handlers: Vec<Box<dyn ServiceEventHandler>>) {
        for handler in handlers {
            self.register_handler(handler);
        }
    }

    /// Remove a handler for a specific service
    pub fn unregister_handler(&mut self, service_name: &str) -> Option<Box<dyn ServiceEventHandler>> {
        debug!("Unregistering event handler for service: {}", service_name);
        self.handlers.remove(service_name)
    }

    /// Check if a handler is registered for a service
    pub fn has_handler(&self, service_name: &str) -> bool {
        self.handlers.contains_key(service_name)
    }

    /// Get the list of services with registered handlers
    pub fn registered_services(&self) -> Vec<String> {
        self.handlers.keys().cloned().collect()
    }

    /// Process a log line and generate service events
    pub async fn process_log_line(&self, service_name: &str, line: &str) -> Option<ServiceEvent> {
        // Try to find a specific handler for this service
        if let Some(handler) = self.handlers.get(service_name) {
            debug!("Processing log line with specific handler for {}: {}", service_name, line.chars().take(50).collect::<String>());
            return handler.handle_log_line(line).await;
        }

        // Fall back to generic handler
        debug!("Processing log line with generic handler for {}: {}", service_name, line.chars().take(50).collect::<String>());
        let fallback = GenericEventHandler::new(service_name.to_string());
        fallback.handle_log_line(line).await
    }

    /// Process a container event and generate service events
    pub async fn process_container_event(&self, service_name: &str, event: &ContainerEvent) -> Option<ServiceEvent> {
        // Try to find a specific handler for this service
        if let Some(handler) = self.handlers.get(service_name) {
            debug!("Processing container event with specific handler for {}: {}", service_name, event.action);
            return handler.handle_container_event(event).await;
        }

        // Fall back to generic handler
        debug!("Processing container event with generic handler for {}: {}", service_name, event.action);
        let fallback = GenericEventHandler::new(service_name.to_string());
        fallback.handle_container_event(event).await
    }

    /// Process multiple log lines in batch
    pub async fn process_log_lines(&self, service_name: &str, lines: &[String]) -> Vec<ServiceEvent> {
        let mut events = Vec::new();
        
        for line in lines {
            if let Some(event) = self.process_log_line(service_name, line).await {
                events.push(event);
            }
        }
        
        events
    }

    /// Get statistics about the registry
    pub fn stats(&self) -> RegistryStats {
        RegistryStats {
            total_handlers: self.handlers.len(),
            registered_services: self.handlers.keys().cloned().collect(),
        }
    }
}

impl Default for ServiceEventRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the service event registry
#[derive(Debug, Clone)]
pub struct RegistryStats {
    /// Total number of registered handlers
    pub total_handlers: usize,
    /// List of services with registered handlers
    pub registered_services: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspection::handlers::PostgresEventHandler;

    #[tokio::test]
    async fn test_registry_basic_operations() {
        let mut registry = ServiceEventRegistry::new();
        
        // Initially no handlers
        assert_eq!(registry.stats().total_handlers, 0);
        assert!(!registry.has_handler("postgres"));
        
        // Register a handler
        registry.register_handler(Box::new(PostgresEventHandler::new()));
        assert_eq!(registry.stats().total_handlers, 1);
        assert!(registry.has_handler("postgres"));
        
        // Unregister handler
        let removed = registry.unregister_handler("postgres");
        assert!(removed.is_some());
        assert_eq!(registry.stats().total_handlers, 0);
        assert!(!registry.has_handler("postgres"));
    }

    #[tokio::test]
    async fn test_log_processing() {
        let mut registry = ServiceEventRegistry::new();
        registry.register_handler(Box::new(PostgresEventHandler::new()));
        
        // Test postgres-specific log processing
        let event = registry.process_log_line("postgres", "database system is ready").await;
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.service_name, "postgres");
        
        // Test fallback for unknown service
        let event = registry.process_log_line("unknown-service", "some log line").await;
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.service_name, "unknown-service");
    }
}