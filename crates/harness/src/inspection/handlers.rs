//! Service event handlers for parsing logs and container events

use async_trait::async_trait;
use regex::Regex;
use std::collections::HashMap;

use super::events::{ServiceEvent, EventType, EventSeverity, ContainerEvent};

/// Trait for handling service-specific event parsing
#[async_trait]
pub trait ServiceEventHandler: Send + Sync {
    /// Parse a log line into a service event
    async fn handle_log_line(&self, line: &str) -> Option<ServiceEvent>;
    
    /// Handle a container event
    async fn handle_container_event(&self, event: &ContainerEvent) -> Option<ServiceEvent>;
    
    /// Get the name of the service this handler manages
    fn service_name(&self) -> &str;
    
    /// Check if this handler can process events from the given service
    fn handles_service(&self, service_name: &str) -> bool {
        self.service_name() == service_name
    }
}

/// Generic event handler that provides basic log parsing for any service
pub struct GenericEventHandler {
    service_name: String,
    error_patterns: Vec<Regex>,
    warning_patterns: Vec<Regex>,
    info_patterns: Vec<Regex>,
}

impl GenericEventHandler {
    /// Create a new generic handler for the specified service
    pub fn new(service_name: String) -> Self {
        let error_patterns = vec![
            Regex::new(r"(?i)(error|failed|failure|exception|panic|fatal)").unwrap(),
            Regex::new(r"exit code: [1-9]").unwrap(),
        ];
        
        let warning_patterns = vec![
            Regex::new(r"(?i)(warn|warning|deprecated)").unwrap(),
            Regex::new(r"(?i)(retry|retrying|timeout)").unwrap(),
        ];
        
        let info_patterns = vec![
            Regex::new(r"(?i)(started|starting|ready|listening|connected)").unwrap(),
            Regex::new(r"(?i)(stopped|stopping|shutdown)").unwrap(),
        ];
        
        Self {
            service_name,
            error_patterns,
            warning_patterns,
            info_patterns,
        }
    }
    
    /// Parse log level and event type from a log line
    fn parse_log_line(&self, line: &str) -> (EventSeverity, EventType) {
        // Check for errors first
        for pattern in &self.error_patterns {
            if pattern.is_match(line) {
                return (EventSeverity::Error, EventType::Error);
            }
        }
        
        // Check for warnings
        for pattern in &self.warning_patterns {
            if pattern.is_match(line) {
                return (EventSeverity::Warning, EventType::StateChange);
            }
        }
        
        // Check for info patterns
        for pattern in &self.info_patterns {
            if pattern.is_match(line) {
                if line.to_lowercase().contains("start") {
                    return (EventSeverity::Info, EventType::Started);
                } else if line.to_lowercase().contains("stop") {
                    return (EventSeverity::Info, EventType::Stopped);
                } else {
                    return (EventSeverity::Info, EventType::StateChange);
                }
            }
        }
        
        // Default to debug info
        (EventSeverity::Debug, EventType::StateChange)
    }
}

#[async_trait]
impl ServiceEventHandler for GenericEventHandler {
    async fn handle_log_line(&self, line: &str) -> Option<ServiceEvent> {
        let (severity, event_type) = self.parse_log_line(line);
        
        // Only emit events for significant occurrences
        if matches!(severity, EventSeverity::Debug) && !event_type.is_problematic() {
            return None;
        }
        
        Some(ServiceEvent::new(
            self.service_name.clone(),
            event_type,
            severity,
            line.trim().to_string(),
        ))
    }
    
    async fn handle_container_event(&self, event: &ContainerEvent) -> Option<ServiceEvent> {
        let (event_type, severity, message) = match event.action.as_str() {
            "start" => (EventType::Started, EventSeverity::Info, "Container started"),
            "stop" | "die" => (EventType::Stopped, EventSeverity::Info, "Container stopped"),
            "kill" => (EventType::Crashed, EventSeverity::Error, "Container killed"),
            "health_status: healthy" => (EventType::HealthCheck, EventSeverity::Info, "Health check passed"),
            "health_status: unhealthy" => (EventType::HealthCheck, EventSeverity::Warning, "Health check failed"),
            _ => return None,
        };
        
        Some(ServiceEvent::new(
            self.service_name.clone(),
            event_type,
            severity,
            message.to_string(),
        ))
    }
    
    fn service_name(&self) -> &str {
        &self.service_name
    }
}

/// Specialized handler for PostgreSQL database events
pub struct PostgresEventHandler {
    connection_pattern: Regex,
    error_pattern: Regex,
}

impl PostgresEventHandler {
    /// Create a new PostgreSQL event handler
    pub fn new() -> Self {
        Self {
            connection_pattern: Regex::new(r"connection (?:received|authorized|authenticated)").unwrap(),
            error_pattern: Regex::new(r"(?i)(error|fatal|panic)").unwrap(),
        }
    }
}

#[async_trait]
impl ServiceEventHandler for PostgresEventHandler {
    async fn handle_log_line(&self, line: &str) -> Option<ServiceEvent> {
        if self.error_pattern.is_match(line) {
            let mut data = HashMap::new();
            if line.contains("connection") {
                data.insert("category".to_string(), serde_json::Value::String("connection".to_string()));
            }
            
            return Some(ServiceEvent::new_with_data(
                "postgres".to_string(),
                EventType::Error,
                EventSeverity::Error,
                line.trim().to_string(),
                serde_json::Value::Object(data.into_iter().collect()),
            ));
        }
        
        if self.connection_pattern.is_match(line) {
            return Some(ServiceEvent::new(
                "postgres".to_string(),
                EventType::Network,
                EventSeverity::Debug,
                line.trim().to_string(),
            ));
        }
        
        if line.contains("database system is ready") {
            return Some(ServiceEvent::new(
                "postgres".to_string(),
                EventType::Started,
                EventSeverity::Info,
                "Database system ready to accept connections".to_string(),
            ));
        }
        
        None
    }
    
    async fn handle_container_event(&self, event: &ContainerEvent) -> Option<ServiceEvent> {
        // Use generic handling for container events
        let generic = GenericEventHandler::new("postgres".to_string());
        generic.handle_container_event(event).await
    }
    
    fn service_name(&self) -> &str {
        "postgres"
    }
}

/// Specialized handler for Graph Node events
pub struct GraphNodeEventHandler {
    indexing_pattern: Regex,
    subgraph_pattern: Regex,
    sync_pattern: Regex,
}

impl GraphNodeEventHandler {
    /// Create a new Graph Node event handler
    pub fn new() -> Self {
        Self {
            indexing_pattern: Regex::new(r"(?i)(indexing|indexed|block|eth_call)").unwrap(),
            subgraph_pattern: Regex::new(r"(?i)(subgraph|deployment|manifest)").unwrap(),
            sync_pattern: Regex::new(r"(?i)(sync|syncing|synced|head|latest)").unwrap(),
        }
    }
}

#[async_trait]
impl ServiceEventHandler for GraphNodeEventHandler {
    async fn handle_log_line(&self, line: &str) -> Option<ServiceEvent> {
        if line.contains("Listening on") {
            return Some(ServiceEvent::new(
                "graph-node".to_string(),
                EventType::Started,
                EventSeverity::Info,
                "Graph Node started and listening".to_string(),
            ));
        }
        
        if self.subgraph_pattern.is_match(line) && line.contains("deployed") {
            return Some(ServiceEvent::new(
                "graph-node".to_string(),
                EventType::StateChange,
                EventSeverity::Info,
                line.trim().to_string(),
            ));
        }
        
        if self.sync_pattern.is_match(line) {
            let mut data = HashMap::new();
            data.insert("category".to_string(), serde_json::Value::String("sync".to_string()));
            
            return Some(ServiceEvent::new_with_data(
                "graph-node".to_string(),
                EventType::StateChange,
                EventSeverity::Debug,
                line.trim().to_string(),
                serde_json::Value::Object(data.into_iter().collect()),
            ));
        }
        
        if self.indexing_pattern.is_match(line) {
            return Some(ServiceEvent::new(
                "graph-node".to_string(),
                EventType::StateChange,
                EventSeverity::Debug,
                line.trim().to_string(),
            ));
        }
        
        None
    }
    
    async fn handle_container_event(&self, event: &ContainerEvent) -> Option<ServiceEvent> {
        let generic = GenericEventHandler::new("graph-node".to_string());
        generic.handle_container_event(event).await
    }
    
    fn service_name(&self) -> &str {
        "graph-node"
    }
}