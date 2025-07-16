//! Service inspection and event streaming for local-network services
//! 
//! This module provides real-time inspection capabilities for services running
//! in the local-network stack. It offers:
//! 
//! - Async event streaming from service logs
//! - Pluggable service-specific event handlers  
//! - Generic fallback for services without custom parsing
//! - Real-time service state monitoring
//! 
//! # Example
//! 
//! ```no_run
//! use local_network_harness::inspection::{ServiceInspector, PostgresEventHandler};
//! 
//! let mut inspector = ServiceInspector::new();
//! inspector.register_handler(Box::new(PostgresEventHandler::new()));
//! 
//! let event_stream = inspector.event_stream();
//! // Stream events in real-time...
//! ```

pub mod events;
pub mod handlers;
pub mod registry;
pub mod streamer;

pub use events::{ServiceEvent, EventType, EventSeverity};
pub use handlers::{ServiceEventHandler, GenericEventHandler, PostgresEventHandler, GraphNodeEventHandler};
pub use registry::ServiceEventRegistry;
pub use streamer::ServiceInspector;