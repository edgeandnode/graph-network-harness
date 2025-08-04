//! Deployment task system for one-time setup operations
//!
//! This module provides traits and utilities for defining deployment tasks that
//! perform one-time setup operations. Tasks are strongly typed with their action
//! and event types, and can leverage command-executor for process management.

use async_channel::Receiver;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use command_executor::event::ProcessEvent;
use command_executor::ProcessEventType;
use async_runtime_compat::Spawner;

use crate::{Error, Result};

/// Trait for deployment tasks that perform one-time setup operations
///
/// Tasks are similar to services but run to completion rather than
/// running continuously. They can be dependencies of services or other tasks.
#[async_trait]
pub trait DeploymentTask: Send + Sync + 'static {
    /// The input type for actions this task can perform
    type Action: DeserializeOwned + Send + JsonSchema;

    /// The event type emitted during task execution
    type Event: Serialize + Send;

    /// The task type identifier that links this implementation to YAML task definitions.
    /// YAML tasks with matching `task_type` will use this implementation.
    fn task_type() -> &'static str
    where
        Self: Sized;

    /// Get the task name
    fn name(&self) -> &str;

    /// Get the task description
    fn description(&self) -> &str;

    /// Check if the task has already been completed (for idempotency)
    ///
    /// This enables tasks to be safely retried without causing duplicate work.
    async fn is_completed(&self) -> Result<bool>;

    /// Execute the task action, returning a stream of events
    ///
    /// The task should check `is_completed()` before performing work to
    /// ensure idempotency.
    async fn execute(&self, action: Self::Action) -> Result<Receiver<Self::Event>>;

    /// Process command executor events into task-specific events
    ///
    /// This allows tasks to provide domain-specific progress information
    /// from generic process output.
    fn process_event(&self, event: &ProcessEvent) -> Option<Self::Event> {
        // Default implementation doesn't translate events
        None
    }

    /// Get the JSON schema for this task's actions
    fn action_schema() -> serde_json::Value
    where
        Self: Sized,
    {
        serde_json::to_value(schemars::schema_for!(Self::Action)).unwrap_or(Value::Null)
    }
}

/// Task execution state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaskState {
    /// Task has not been started
    NotStarted,
    /// Task is currently running
    Running,
    /// Task completed successfully
    Completed,
    /// Task failed with an error
    Failed(String),
}

impl TaskState {
    /// Check if the task is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self, TaskState::Completed | TaskState::Failed(_))
    }

    /// Check if the task completed successfully
    pub fn is_completed(&self) -> bool {
        matches!(self, TaskState::Completed)
    }
}

/// Result of a task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// The final state of the task
    pub state: TaskState,
    /// Optional output data from the task
    pub output: Option<Value>,
    /// Metadata about the execution
    pub metadata: HashMap<String, String>,
}

/// Wrapper that adds JSON serialization to any DeploymentTask
///
/// This wrapper handles conversion between JSON and typed values,
/// allowing tasks to work with their native types while supporting
/// wire protocol communication.
pub struct TaskWrapper<T>
where
    T: DeploymentTask,
    T::Action: JsonSchema,
    T::Event: JsonSchema,
{
    inner: T,
    action_schema: Value,
    event_schema: Value,
}

impl<T> TaskWrapper<T>
where
    T: DeploymentTask,
    T::Action: JsonSchema,
    T::Event: JsonSchema,
{
    /// Create a new task wrapper
    pub fn new(task: T) -> Self {
        let action_schema = schemars::schema_for!(T::Action);
        let event_schema = schemars::schema_for!(T::Event);

        Self {
            inner: task,
            action_schema: serde_json::to_value(action_schema).unwrap(),
            event_schema: serde_json::to_value(event_schema).unwrap(),
        }
    }

    /// Execute a task using JSON input, returning typed events
    pub async fn execute_json(&self, input: Value) -> Result<Receiver<T::Event>> {
        // Deserialize JSON to typed action
        let action: T::Action = serde_json::from_value(input)
            .map_err(|e| Error::service_type(format!("Failed to deserialize action: {}", e)))?;

        // Execute the typed action and return the event receiver directly
        self.inner.execute(action).await
    }
}

/// Trait for tasks that work with JSON (used for dynamic dispatch)
#[async_trait]
pub trait JsonTask: Send + Sync {
    /// Get the task name
    fn name(&self) -> &str;

    /// Get the task description
    fn description(&self) -> &str;

    /// Check if the task is completed
    async fn is_completed(&self) -> Result<bool>;

    /// Execute the task using JSON input
    /// Returns a receiver for JSON events and a future that must be spawned to perform the conversion
    async fn execute_json(&self, input: Value) -> Result<(Receiver<Value>, Pin<Box<dyn Future<Output = ()> + Send>>)>;

    /// Get the action schema
    fn action_schema(&self) -> &Value;

    /// Get the event schema
    fn event_schema(&self) -> &Value;
}

/// Blanket implementation for wrapped tasks
#[async_trait]
impl<T> JsonTask for TaskWrapper<T>
where
    T: DeploymentTask + 'static,
    T::Action: JsonSchema,
    T::Event: JsonSchema,
{
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    async fn is_completed(&self) -> Result<bool> {
        self.inner.is_completed().await
    }

    async fn execute_json(&self, input: Value) -> Result<(Receiver<Value>, Pin<Box<dyn Future<Output = ()> + Send>>)> {
        // Get typed event receiver
        let event_rx = self.execute_json(input).await?;
        
        // Create channel for JSON events
        let (tx, rx) = async_channel::unbounded();
        
        // Create the conversion future
        let converter = async move {
            while let Ok(event) = event_rx.recv().await {
                if let Ok(json) = serde_json::to_value(&event) {
                    let _ = tx.send(json).await;
                }
            }
        };
        
        Ok((rx, Box::pin(converter)))
    }

    fn action_schema(&self) -> &Value {
        &self.action_schema
    }

    fn event_schema(&self) -> &Value {
        &self.event_schema
    }
}

/// Stack of deployment tasks
///
/// The TaskStack manages a collection of tasks that can perform one-time
/// setup operations. Tasks are stored as trait objects to allow different task types.
pub struct TaskStack {
    tasks: HashMap<String, Box<dyn JsonTask>>,
    task_types: HashSet<String>,
}

impl TaskStack {
    /// Create a new empty task stack
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            task_types: HashSet::new(),
        }
    }

    /// Register a task in the stack
    ///
    /// The task must implement JsonSchema for its Action and Event types.
    pub fn register<T>(&mut self, instance_name: String, task: T) -> Result<()>
    where
        T: DeploymentTask + 'static,
        T::Action: JsonSchema,
        T::Event: JsonSchema,
    {
        if self.tasks.contains_key(&instance_name) {
            return Err(Error::service_type(format!(
                "Task instance '{}' already registered",
                instance_name
            )));
        }

        // Track the task type
        self.task_types.insert(T::task_type().to_string());

        let wrapper = TaskWrapper::new(task);
        self.tasks.insert(instance_name, Box::new(wrapper));
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
        input: Value,
        spawner: &S,
    ) -> Result<Receiver<Value>> {
        let task = self.get(instance_name).ok_or_else(|| {
            Error::service_type(format!("Task instance '{}' not found", instance_name))
        })?;

        let (rx, converter) = task.execute_json(input).await?;
        
        // Spawn the converter
        spawner.spawn(converter);
        
        Ok(rx)
    }

    /// Check if a task is completed
    pub async fn is_completed(&self, instance_name: &str) -> Result<bool> {
        let task = self.get(instance_name).ok_or_else(|| {
            Error::service_type(format!("Task instance '{}' not found", instance_name))
        })?;

        task.is_completed().await
    }
}

impl Default for TaskStack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    use async_runtime_compat::smol::SmolSpawner;
    use chrono;

    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    struct TestAction {
        message: String,
    }

    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    struct TestEvent {
        progress: u8,
        status: String,
    }

    struct TestTask {
        completed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl TestTask {
        fn new() -> Self {
            Self {
                completed: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            }
        }
    }

    #[async_trait]
    impl DeploymentTask for TestTask {
        type Action = TestAction;
        type Event = TestEvent;

        fn task_type() -> &'static str {
            "test-task"
        }

        fn name(&self) -> &str {
            "test-task"
        }

        fn description(&self) -> &str {
            "A test deployment task"
        }

        async fn is_completed(&self) -> Result<bool> {
            Ok(self.completed.load(std::sync::atomic::Ordering::SeqCst))
        }

        async fn execute(&self, action: Self::Action) -> Result<Receiver<Self::Event>> {
            let (tx, rx) = async_channel::bounded(10);
            let completed = self.completed.clone();

            // Simulate task execution using smol since we're in tests
            smol::spawn(async move {
                for i in 0..=100 {
                    let _ = tx
                        .send(TestEvent {
                            progress: i,
                            status: format!("Processing: {}", action.message),
                        })
                        .await;
                    if i % 25 == 0 {
                        smol::Timer::after(std::time::Duration::from_millis(10)).await;
                    }
                }
                completed.store(true, std::sync::atomic::Ordering::SeqCst);
            }).detach();

            Ok(rx)
        }
    }

    #[test]
    fn test_task_stack_registration() {
        let mut stack = TaskStack::new();
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
        let mut stack = TaskStack::new();
        stack.register("test-1".to_string(), TestTask::new()).unwrap();

        let input = serde_json::json!({
            "message": "Hello, Task!"
        });

        let spawner = SmolSpawner;
        let rx = stack.execute("test-1", input, &spawner).await.unwrap();
        
        // Collect some events
        let mut events = Vec::new();
        for _ in 0..5 {
            if let Ok(event) = rx.recv().await {
                events.push(event);
            }
        }

        assert!(!events.is_empty());
        assert_eq!(events[0]["progress"], 0);
    }

    #[smol_potat::test]
    async fn test_task_completion() {
        let mut stack = TaskStack::new();
        let task = TestTask::new();
        stack.register("test-1".to_string(), task).unwrap();

        // Initially not completed
        assert!(!stack.is_completed("test-1").await.unwrap());

        // Execute the task
        let input = serde_json::json!({ "message": "Test" });
        let spawner = SmolSpawner;
        let rx = stack.execute("test-1", input, &spawner).await.unwrap();

        // Drain all events to ensure task completes
        while rx.recv().await.is_ok() {}

        // Small delay to ensure completion flag is set
        smol::Timer::after(std::time::Duration::from_millis(50)).await;

        // Should now be completed
        assert!(stack.is_completed("test-1").await.unwrap());
    }

    #[test]
    fn test_task_type_tracking() {
        let mut stack = TaskStack::new();
        
        // Register multiple instances of the same task type
        stack.register("test-1".to_string(), TestTask::new()).unwrap();
        stack.register("test-2".to_string(), TestTask::new()).unwrap();
        
        // Should only have one task type
        let types = stack.list_types();
        assert_eq!(types.len(), 1);
        assert!(types.contains(&"test-task"));
    }

    // Test for process event translation
    struct EventTranslatingTask {
        completed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    struct TranslatingAction {
        command: String,
    }

    #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
    struct TranslatingEvent {
        event_type: String,
        message: String,
    }

    #[async_trait]
    impl DeploymentTask for EventTranslatingTask {
        type Action = TranslatingAction;
        type Event = TranslatingEvent;

        fn task_type() -> &'static str {
            "event-translating-task"
        }

        fn name(&self) -> &str {
            "event-translator"
        }

        fn description(&self) -> &str {
            "Task that translates process events"
        }

        async fn is_completed(&self) -> Result<bool> {
            Ok(self.completed.load(std::sync::atomic::Ordering::SeqCst))
        }

        async fn execute(&self, _action: Self::Action) -> Result<Receiver<Self::Event>> {
            let (tx, rx) = async_channel::bounded(10);
            let completed = self.completed.clone();

            // Use smol for tests
            smol::spawn(async move {
                // Send some events
                let _ = tx.send(TranslatingEvent {
                    event_type: "started".to_string(),
                    message: "Task started".to_string(),
                }).await;
                
                smol::Timer::after(std::time::Duration::from_millis(10)).await;
                
                let _ = tx.send(TranslatingEvent {
                    event_type: "completed".to_string(),
                    message: "Task completed".to_string(),
                }).await;
                
                completed.store(true, std::sync::atomic::Ordering::SeqCst);
            }).detach();

            Ok(rx)
        }

        fn process_event(&self, event: &ProcessEvent) -> Option<Self::Event> {
            match &event.event_type {
                ProcessEventType::Stdout => {
                    if let Some(data) = &event.data {
                        if data.contains("SUCCESS") {
                            Some(TranslatingEvent {
                                event_type: "success".to_string(),
                                message: data.clone(),
                            })
                        } else if data.contains("ERROR") {
                            Some(TranslatingEvent {
                                event_type: "error".to_string(),
                                message: data.clone(),
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                ProcessEventType::Exited { code, .. } => {
                    if let Some(code) = code {
                        Some(TranslatingEvent {
                            event_type: "exit".to_string(),
                            message: format!("Process exited with code {}", code),
                        })
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
    }

    #[test]
    fn test_process_event_translation() {
        let task = EventTranslatingTask {
            completed: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };

        // Test stdout success
        let event = ProcessEvent {
            timestamp: chrono::Utc::now(),
            event_type: ProcessEventType::Stdout,
            data: Some("Operation completed with SUCCESS".to_string()),
        };
        
        let result = task.process_event(&event);
        assert_eq!(result, Some(TranslatingEvent {
            event_type: "success".to_string(),
            message: "Operation completed with SUCCESS".to_string(),
        }));

        // Test stdout error
        let event = ProcessEvent {
            timestamp: chrono::Utc::now(),
            event_type: ProcessEventType::Stdout,
            data: Some("ERROR: Failed to connect".to_string()),
        };
        
        let result = task.process_event(&event);
        assert_eq!(result, Some(TranslatingEvent {
            event_type: "error".to_string(),
            message: "ERROR: Failed to connect".to_string(),
        }));

        // Test exit code
        let event = ProcessEvent {
            timestamp: chrono::Utc::now(),
            event_type: ProcessEventType::Exited { code: Some(1), signal: None },
            data: None,
        };
        
        let result = task.process_event(&event);
        assert_eq!(result, Some(TranslatingEvent {
            event_type: "exit".to_string(),
            message: "Process exited with code 1".to_string(),
        }));

        // Test no translation
        let event = ProcessEvent {
            timestamp: chrono::Utc::now(),
            event_type: ProcessEventType::Stdout,
            data: Some("Regular output".to_string()),
        };
        
        let result = task.process_event(&event);
        assert_eq!(result, None);
    }

    #[smol_potat::test]
    async fn test_end_to_end_task_execution() {
        let mut stack = TaskStack::new();
        let task = EventTranslatingTask {
            completed: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        
        stack.register("translator".to_string(), task).unwrap();

        // Execute the task
        let input = serde_json::json!({
            "command": "test-command"
        });

        let spawner = SmolSpawner;
        let rx = stack.execute("translator", input, &spawner).await.unwrap();

        // Collect all events
        let mut events = Vec::new();
        while let Ok(event) = rx.recv().await {
            events.push(event);
        }

        // Verify we got the expected events
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["event_type"], "started");
        assert_eq!(events[1]["event_type"], "completed");

        // Verify task is completed
        assert!(stack.is_completed("translator").await.unwrap());
    }

    #[smol_potat::test]
    async fn test_task_error_handling() {
        let stack = TaskStack::new();

        // Try to execute non-existent task
        let input = serde_json::json!({});
        let spawner = SmolSpawner;
        let result = stack.execute("non-existent", input, &spawner).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));

        // Try to check completion of non-existent task
        let result = stack.is_completed("non-existent").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_json_schema_generation() {
        let schema = TestTask::action_schema();
        assert!(!schema.is_null());
        
        // Verify schema contains expected fields
        let schema_str = schema.to_string();
        assert!(schema_str.contains("TestAction"));
        assert!(schema_str.contains("message"));
    }

    #[test]
    fn test_task_state_transitions() {
        let state = TaskState::NotStarted;
        assert!(!state.is_terminal());
        assert!(!state.is_completed());

        let state = TaskState::Running;
        assert!(!state.is_terminal());
        assert!(!state.is_completed());

        let state = TaskState::Completed;
        assert!(state.is_terminal());
        assert!(state.is_completed());

        let state = TaskState::Failed("error".to_string());
        assert!(state.is_terminal());
        assert!(!state.is_completed());
    }
}