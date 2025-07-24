//! Shared protocol types between client and daemon

// Re-export the daemon protocol types
pub use crate::daemon::handlers::{DaemonRequest, DaemonResponse, ServiceInfo};