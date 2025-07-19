//! Raw process events and log filtering

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A raw event from a process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessEvent {
    /// When the event occurred
    pub timestamp: DateTime<Utc>,
    /// The type of event
    pub event_type: ProcessEventType,
    /// Optional data associated with the event
    pub data: Option<String>,
}

impl ProcessEvent {
    /// Create a new process event
    pub fn new(event_type: ProcessEventType) -> Self {
        Self {
            timestamp: Utc::now(),
            event_type,
            data: None,
        }
    }
    
    /// Create a new process event with data
    pub fn new_with_data(event_type: ProcessEventType, data: String) -> Self {
        Self {
            timestamp: Utc::now(),
            event_type,
            data: Some(data),
        }
    }
}

/// Types of raw process events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProcessEventType {
    /// Process has started
    Started { pid: u32 },
    /// Process has exited
    Exited { code: Option<i32>, signal: Option<i32> },
    /// Log line from stdout
    Stdout,
    /// Log line from stderr  
    Stderr,
}

/// Filter for process log output
pub trait LogFilter: Send + Sync {
    /// Filter a log line, returning None to drop it
    /// 
    /// The returned &str can be the same as the input (pass-through)
    /// or a substring of it (partial filtering).
    fn filter<'a>(&self, line: &'a str, source: LogSource) -> Option<&'a str>;
}

/// Source of a log line
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSource {
    /// Standard output
    Stdout,
    /// Standard error
    Stderr,
}

/// A no-op filter that passes all logs through
pub struct NoOpFilter;

impl LogFilter for NoOpFilter {
    fn filter<'a>(&self, line: &'a str, _source: LogSource) -> Option<&'a str> {
        Some(line)
    }
}