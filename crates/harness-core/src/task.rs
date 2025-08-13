//! Deployment task system for one-time setup operations
//!
//! This module provides traits and utilities for defining deployment tasks that
//! perform one-time setup operations. Tasks are strongly typed with their action
//! and event types, and can leverage command-executor for process management.

use async_channel::Receiver;
use async_runtime_compat::Spawner;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;

use crate::Error;
use std::result::Result;

/// Simplified trait for deployment tasks using state machines
///
/// Tasks run to completion and stream their state machine states.
/// The state itself indicates progress and completion.
#[async_trait]
pub trait DeploymentTask: Send + Sync + 'static {
    /// The state machine state type that represents task progress
    type State: Send + Serialize + Clone + PartialEq;

    /// The task type identifier that links this implementation to YAML task definitions
    const TASK_TYPE: &'static str;

    /// Execute the task, returning a stream of state changes
    ///
    /// The task runs its internal state machine and emits state transitions
    /// as they occur. The final state indicates completion or failure.
    async fn execute(&self) -> Result<Receiver<Self::State>, Error>;
}

/// Adapter that adds JSON serialization to any DeploymentTask
///
/// This adapter handles conversion between typed states and JSON,
/// allowing tasks to work with their native types while supporting
/// wire protocol communication.
pub struct JsonTaskAdapter<T>
where
    T: DeploymentTask,
    T::State: JsonSchema,
{
    inner: T,
    state_schema: Value,
}

impl<T> JsonTaskAdapter<T>
where
    T: DeploymentTask,
    T::State: JsonSchema,
{
    /// Create a new task wrapper
    pub fn new(task: T) -> Self {
        let state_schema = schemars::schema_for!(T::State);

        Self {
            inner: task,
            state_schema: serde_json::to_value(state_schema).unwrap(),
        }
    }

    /// Execute the task, returning a receiver for JSON state updates and a converter future
    pub async fn execute_json(
        &self,
    ) -> Result<(Receiver<Value>, Pin<Box<dyn Future<Output = ()> + Send>>), Error> {
        let state_rx = self.inner.execute().await?;
        let (tx, rx) = async_channel::unbounded();

        // Create a future to convert states to JSON
        let converter = async move {
            while let Ok(state) = state_rx.recv().await {
                if let Ok(json) = serde_json::to_value(&state) {
                    let _ = tx.send(json).await;
                }
            }
        };

        Ok((rx, Box::pin(converter)))
    }
}

/// Trait for tasks that work with JSON (used for dynamic dispatch)
#[async_trait]
pub trait JsonTask: Send + Sync {
    /// Execute the task, returning a stream of JSON state updates and a converter future
    async fn execute_json(
        &self,
    ) -> Result<(Receiver<Value>, Pin<Box<dyn Future<Output = ()> + Send>>), Error>;

    /// Get the state schema
    fn state_schema(&self) -> &Value;
}

/// Blanket implementation for JSON task adapter
#[async_trait]
impl<T> JsonTask for JsonTaskAdapter<T>
where
    T: DeploymentTask + 'static,
    T::State: JsonSchema,
{
    async fn execute_json(
        &self,
    ) -> Result<(Receiver<Value>, Pin<Box<dyn Future<Output = ()> + Send>>), Error> {
        self.execute_json().await
    }

    fn state_schema(&self) -> &Value {
        &self.state_schema
    }
}

/// Registry of JSON-wrapped deployment tasks
///
/// The JsonTaskRegistry manages a collection of tasks that have been wrapped
/// to provide a JSON interface. Tasks are stored as trait objects to allow
/// different task types with unified JSON-based interaction.
pub struct JsonTaskRegistry {
    tasks: HashMap<String, Box<dyn JsonTask>>,
    task_types: HashSet<String>,
}

impl JsonTaskRegistry {
    /// Create a new empty task stack
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            task_types: HashSet::new(),
        }
    }

    /// Register a task in the stack
    ///
    /// The task must implement JsonSchema for its State type.
    pub fn register<T>(&mut self, instance_name: String, task: T) -> Result<(), Error>
    where
        T: DeploymentTask + 'static,
        T::State: JsonSchema,
    {
        if self.tasks.contains_key(&instance_name) {
            return Err(Error::service_type(format!(
                "Task instance '{instance_name}' already registered"
            )));
        }

        // Track the task type
        self.task_types.insert(T::TASK_TYPE.to_string());

        let adapter = JsonTaskAdapter::new(task);
        self.tasks.insert(instance_name, Box::new(adapter));
        Ok(())
    }

    /// Get a task by instance name
    pub fn get(&self, instance_name: &str) -> Option<&dyn JsonTask> {
        self.tasks.get(instance_name).map(|t| t.as_ref())
    }

    /// List all registered task instances
    pub fn list(&self) -> Vec<(&str, &dyn JsonTask)> {
        self.tasks
            .iter()
            .map(|(name, task)| (name.as_str(), task.as_ref()))
            .collect()
    }

    /// Get all task type identifiers
    pub fn list_types(&self) -> Vec<&str> {
        self.task_types.iter().map(|s| s.as_str()).collect()
    }

    /// Execute a task
    pub async fn execute<S: Spawner>(
        &self,
        instance_name: &str,
        spawner: &S,
    ) -> Result<Receiver<Value>, Error> {
        let task = self.get(instance_name).ok_or_else(|| {
            Error::service_type(format!("Task instance '{instance_name}' not found"))
        })?;

        let (rx, converter) = task.execute_json().await?;
        spawner.spawn(converter);
        Ok(rx)
    }
}

impl Default for JsonTaskRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    // Simple test state for testing
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
    enum TestTaskState {
        Idle,
        Running,
        Completed,
        Failed,
    }

    struct TestTask {
        state: std::sync::Arc<std::sync::Mutex<TestTaskState>>,
    }

    impl TestTask {
        fn new() -> Self {
            Self {
                state: std::sync::Arc::new(std::sync::Mutex::new(TestTaskState::Idle)),
            }
        }
    }

    #[async_trait]
    impl DeploymentTask for TestTask {
        type State = TestTaskState;

        const TASK_TYPE: &'static str = "test-task";

        async fn execute(&self) -> Result<Receiver<Self::State>, Error> {
            let (tx, rx) = async_channel::bounded(10);
            let state = self.state.clone();

            // Simulate task execution using smol since we're in tests
            smol::spawn(async move {
                // Send state transitions
                let _ = tx.send(TestTaskState::Running).await;
                if let Ok(mut s) = state.lock() {
                    *s = TestTaskState::Running;
                }

                smol::Timer::after(std::time::Duration::from_millis(10)).await;

                let _ = tx.send(TestTaskState::Completed).await;
                if let Ok(mut s) = state.lock() {
                    *s = TestTaskState::Completed;
                }
            })
            .detach();

            Ok(rx)
        }
    }

    #[test]
    fn test_task_stack_registration() {
        let mut stack = JsonTaskRegistry::new();
        let task = TestTask::new();

        // Register task
        stack.register("test-1".to_string(), task).unwrap();

        // Check it's registered
        assert!(stack.get("test-1").is_some());
        assert_eq!(stack.list().len(), 1);

        // Try to register with same name (should fail)
        let result = stack.register("test-1".to_string(), TestTask::new());
        assert!(result.is_err());
    }

    #[smol_potat::test]
    async fn test_task_execution() {
        use async_runtime_compat::smol::SmolSpawner;

        let mut stack = JsonTaskRegistry::new();
        stack
            .register("test-1".to_string(), TestTask::new())
            .unwrap();

        let spawner = SmolSpawner;
        let rx = stack.execute("test-1", &spawner).await.unwrap();

        // Collect state transitions
        let mut states = Vec::new();
        while let Ok(state) = rx.recv().await {
            states.push(state);
        }

        assert!(!states.is_empty());
        // Should have transitioned through Running to Completed
        assert!(states.iter().any(|s| {
            if let Ok(state) = serde_json::from_value::<TestTaskState>(s.clone()) {
                state == TestTaskState::Completed
            } else {
                false
            }
        }));
    }

    #[smol_potat::test]
    async fn test_task_state() {
        use async_runtime_compat::smol::SmolSpawner;

        let mut stack = JsonTaskRegistry::new();
        let task = TestTask::new();
        stack.register("test-1".to_string(), task).unwrap();

        // Execute the task
        let spawner = SmolSpawner;
        let mut rx = stack.execute("test-1", &spawner).await.unwrap();

        // First state should be Running
        let state = rx.recv().await.unwrap();
        let state: TestTaskState = serde_json::from_value(state).unwrap();
        assert_eq!(state, TestTaskState::Running);

        // Next state should be Completed
        let state = rx.recv().await.unwrap();
        let state: TestTaskState = serde_json::from_value(state).unwrap();
        assert_eq!(state, TestTaskState::Completed);
    }

    #[test]
    fn test_task_type_tracking() {
        let mut stack = JsonTaskRegistry::new();

        // Register multiple instances of the same task type
        stack
            .register("test-1".to_string(), TestTask::new())
            .unwrap();
        stack
            .register("test-2".to_string(), TestTask::new())
            .unwrap();

        // Should only have one task type
        let types = stack.list_types();
        assert_eq!(types.len(), 1);
        assert!(types.contains(&"test-task"));
    }

    #[smol_potat::test]
    async fn test_task_error_handling() {
        use async_runtime_compat::smol::SmolSpawner;

        let stack = JsonTaskRegistry::new();
        let spawner = SmolSpawner;

        // Try to execute non-existent task
        let result = stack.execute("non-existent", &spawner).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));

        // Try to execute non-existent task (should error)
        // The execute method already checked and returned error above
    }
}
