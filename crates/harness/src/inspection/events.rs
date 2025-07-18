//! Core event types for service inspection

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A structured event from a service in the local-network stack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEvent {
    /// When the event occurred
    pub timestamp: DateTime<Utc>,
    /// Name of the service that generated the event
    pub service_name: String,
    /// Type of event
    pub event_type: EventType,
    /// Severity level of the event
    pub severity: EventSeverity,
    /// Human-readable message describing the event
    pub message: String,
    /// Service-specific structured data
    pub data: serde_json::Value,
}

impl ServiceEvent {
    /// Create a new service event
    pub fn new(
        service_name: String,
        event_type: EventType,
        severity: EventSeverity,
        message: String,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            service_name,
            event_type,
            severity,
            message,
            data: serde_json::Value::Null,
        }
    }

    /// Create a new service event with structured data
    pub fn new_with_data(
        service_name: String,
        event_type: EventType,
        severity: EventSeverity,
        message: String,
        data: serde_json::Value,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            service_name,
            event_type,
            severity,
            message,
            data,
        }
    }

    /// Check if this event indicates an error condition
    pub fn is_error(&self) -> bool {
        matches!(self.severity, EventSeverity::Error | EventSeverity::Critical)
    }

    /// Check if this event is from a specific service
    pub fn is_from_service(&self, service_name: &str) -> bool {
        self.service_name == service_name
    }
}

/// Types of events that can occur in services
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventType {
    /// Service started or is starting up
    Started,
    /// Service stopped or is shutting down
    Stopped,
    /// Service crashed or exited unexpectedly
    Crashed,
    /// Error occurred during operation
    Error,
    /// Health check status update
    HealthCheck,
    /// Service state changed
    StateChange,
    /// Configuration was updated
    ConfigUpdate,
    /// Network-related event
    Network,
    /// Database-related event
    Database,
    /// Performance metrics update
    Metrics,
    /// Custom service-specific event type
    Custom(String),
}

impl EventType {
    /// Check if this event type indicates a problem
    pub fn is_problematic(&self) -> bool {
        matches!(self, EventType::Error | EventType::Crashed)
    }
}

/// Severity levels for events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventSeverity {
    /// Detailed information for debugging
    Trace,
    /// General information
    Debug,
    /// Normal operational information
    Info,
    /// Warning about potential issues
    Warning,
    /// Error that doesn't stop operation
    Error,
    /// Critical error that may cause service failure
    Critical,
}

impl EventSeverity {
    /// Check if this severity level should be highlighted
    pub fn needs_attention(&self) -> bool {
        matches!(self, EventSeverity::Warning | EventSeverity::Error | EventSeverity::Critical)
    }
}

/// Container event from Docker daemon
#[derive(Debug, Clone)]
pub struct ContainerEvent {
    /// Container ID
    pub container_id: String,
    /// Container name
    pub container_name: String,
    /// Event action (start, stop, die, etc.)
    pub action: String,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Additional event attributes
    pub attributes: std::collections::HashMap<String, String>,
}

impl ContainerEvent {
    /// Create a new container event
    pub fn new(
        container_id: String,
        container_name: String,
        action: String,
    ) -> Self {
        Self {
            container_id,
            container_name,
            action,
            timestamp: Utc::now(),
            attributes: std::collections::HashMap::new(),
        }
    }

    /// Check if this is a start event
    pub fn is_start(&self) -> bool {
        self.action == "start"
    }

    /// Check if this is a stop event  
    pub fn is_stop(&self) -> bool {
        matches!(self.action.as_str(), "stop" | "die" | "kill")
    }

    /// Check if this indicates a health check
    pub fn is_health_check(&self) -> bool {
        self.action.contains("health")
    }
}