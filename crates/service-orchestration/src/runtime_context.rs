//! Runtime context for task and service execution
//!
//! Provides access to runtime state including completed task outputs,
//! allocated ports, and other dynamic values that aren't known at wire time.

use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Runtime context passed to tasks and services at execution time
///
/// Contains dynamic state that becomes available as the stack executes:
/// - Task outputs (contract addresses, deployment IDs, etc.)
/// - Allocated ports for services
/// - Environment overrides
///
/// This context is the bridge between task outputs and dependent task inputs,
/// and will be exposed to scripting in the future.
#[derive(Debug, Clone)]
pub struct RuntimeContext {
    /// Internal shared state (allows context to be cloned while sharing data)
    inner: Arc<RwLock<RuntimeContextInner>>,
}

#[derive(Debug, Default)]
struct RuntimeContextInner {
    /// Outputs from completed tasks, keyed by task name
    /// Each task's outputs are a map of output name -> value
    task_outputs: HashMap<String, HashMap<String, JsonValue>>,

    /// Allocated ports for services, keyed by service name
    /// Each service's ports are a map of port name -> port number
    allocated_ports: HashMap<String, HashMap<String, u16>>,
}

impl Default for RuntimeContext {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeContext {
    /// Create a new empty runtime context
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(RuntimeContextInner::default())),
        }
    }

    /// Record outputs from a completed task
    pub fn set_task_outputs(&self, task_name: &str, outputs: HashMap<String, JsonValue>) {
        let mut inner = self.inner.write().unwrap();
        inner.task_outputs.insert(task_name.to_string(), outputs);
    }

    /// Get a specific output from a completed task
    pub fn task_output(&self, task_name: &str, output_key: &str) -> Option<JsonValue> {
        let inner = self.inner.read().unwrap();
        inner
            .task_outputs
            .get(task_name)
            .and_then(|outputs| outputs.get(output_key).cloned())
    }

    /// Get a task output as a string (convenience for contract addresses etc.)
    pub fn task_output_str(&self, task_name: &str, output_key: &str) -> Option<String> {
        self.task_output(task_name, output_key)
            .and_then(|v| match v {
                JsonValue::String(s) => Some(s),
                other => Some(other.to_string().trim_matches('"').to_string()),
            })
    }

    /// Get all outputs from a completed task
    pub fn task_outputs(&self, task_name: &str) -> Option<HashMap<String, JsonValue>> {
        let inner = self.inner.read().unwrap();
        inner.task_outputs.get(task_name).cloned()
    }

    /// Check if a task has completed (has outputs recorded)
    pub fn has_task_outputs(&self, task_name: &str) -> bool {
        let inner = self.inner.read().unwrap();
        inner.task_outputs.contains_key(task_name)
    }

    /// Record allocated ports for a service
    pub fn set_allocated_ports(&self, service_name: &str, ports: HashMap<String, u16>) {
        let mut inner = self.inner.write().unwrap();
        inner
            .allocated_ports
            .insert(service_name.to_string(), ports);
    }

    /// Get an allocated port for a service
    pub fn allocated_port(&self, service_name: &str, port_name: &str) -> Option<u16> {
        let inner = self.inner.read().unwrap();
        inner
            .allocated_ports
            .get(service_name)
            .and_then(|ports| ports.get(port_name).copied())
    }

    /// Get all allocated ports for a service
    pub fn allocated_ports(&self, service_name: &str) -> Option<HashMap<String, u16>> {
        let inner = self.inner.read().unwrap();
        inner.allocated_ports.get(service_name).cloned()
    }

    /// List all tasks that have recorded outputs
    pub fn completed_tasks(&self) -> Vec<String> {
        let inner = self.inner.read().unwrap();
        inner.task_outputs.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_outputs() {
        let ctx = RuntimeContext::new();

        // No outputs initially
        assert!(ctx.task_output("deploy-contracts", "GraphToken").is_none());
        assert!(!ctx.has_task_outputs("deploy-contracts"));

        // Set outputs
        let mut outputs = HashMap::new();
        outputs.insert(
            "GraphToken".to_string(),
            JsonValue::String("0x1234".to_string()),
        );
        outputs.insert(
            "L1Staking".to_string(),
            JsonValue::String("0x5678".to_string()),
        );
        ctx.set_task_outputs("deploy-contracts", outputs);

        // Query outputs
        assert!(ctx.has_task_outputs("deploy-contracts"));
        assert_eq!(
            ctx.task_output_str("deploy-contracts", "GraphToken"),
            Some("0x1234".to_string())
        );
        assert_eq!(
            ctx.task_output_str("deploy-contracts", "L1Staking"),
            Some("0x5678".to_string())
        );
        assert!(ctx.task_output("deploy-contracts", "Unknown").is_none());
    }

    #[test]
    fn test_allocated_ports() {
        let ctx = RuntimeContext::new();

        let mut ports = HashMap::new();
        ports.insert("rpc".to_string(), 8545u16);
        ports.insert("ws".to_string(), 8546u16);
        ctx.set_allocated_ports("anvil", ports);

        assert_eq!(ctx.allocated_port("anvil", "rpc"), Some(8545));
        assert_eq!(ctx.allocated_port("anvil", "ws"), Some(8546));
        assert!(ctx.allocated_port("anvil", "admin").is_none());
        assert!(ctx.allocated_port("other", "rpc").is_none());
    }

    #[test]
    fn test_context_is_cloneable_and_shared() {
        let ctx1 = RuntimeContext::new();
        let ctx2 = ctx1.clone();

        // Set via ctx1
        let mut outputs = HashMap::new();
        outputs.insert("key".to_string(), JsonValue::String("value".to_string()));
        ctx1.set_task_outputs("task1", outputs);

        // Read via ctx2 (shared state)
        assert_eq!(
            ctx2.task_output_str("task1", "key"),
            Some("value".to_string())
        );
    }
}
