//! Task manager for orchestrating one-time deployment tasks.
//!
//! The TaskManager handles execution of tasks like contract deployments,
//! database migrations, and other one-time setup operations.
//!
//! # Task Types
//!
//! The TaskManager supports two categories of tasks:
//!
//! 1. **Config-based tasks**: Simple shell commands defined in YAML configuration.
//!    These are executed via `TaskExecutor` implementations like `ProcessTaskExecutor`.
//!
//! 2. **Typed tasks**: Complex state-machine-based tasks implemented in Rust code.
//!    These are executed via a `TypedTaskProvider` that abstracts the task registry.

use crate::OrchestrationError;
use async_channel::Receiver;
use async_runtime_compat::Spawner;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::result::Result;
use std::sync::{Arc, RwLock};
use tracing::{debug, warn};

/// Task execution status
#[derive(Debug, Clone)]
pub enum TaskStatus {
    /// Task has not been started
    NotStarted,
    /// Task is currently executing
    Running,
    /// Task completed successfully
    Completed,
    /// Task failed with an error
    Failed(String),
}

/// Information about a running or completed task
#[derive(Debug, Clone)]
pub struct TaskExecution {
    /// Task name
    pub name: String,
    /// Current status
    pub status: TaskStatus,
    /// State receiver for monitoring task progress
    pub state_receiver: Option<Receiver<JsonValue>>,
    /// Task outputs (populated on completion for tasks that produce outputs)
    ///
    /// Contract deployment tasks store addresses here, e.g.:
    /// - "GraphToken" -> "0x..."
    /// - "L1Staking" -> "0x..."
    pub outputs: HashMap<String, JsonValue>,
}

/// Task executor trait - implemented by specific executors for different execution backends
#[async_trait::async_trait]
pub trait TaskExecutor: Send + Sync {
    /// Check if this executor can handle the given task configuration
    fn can_handle(&self, config: &crate::TaskConfig) -> bool;

    /// Check if a task has already been completed
    async fn is_complete(&self, name: &str, config: &crate::TaskConfig) -> Result<bool, OrchestrationError>;

    /// Execute a task
    async fn execute(
        &self,
        name: &str,
        config: &crate::TaskConfig,
        spawner: &dyn Spawner,
    ) -> Result<Receiver<JsonValue>, OrchestrationError>;
}

/// Provider for typed deployment tasks
///
/// This trait abstracts access to typed tasks (state-machine-based Rust implementations)
/// from the TaskManager. Higher-level crates can implement this trait to integrate
/// their task registries without creating circular dependencies.
///
/// Typed tasks are checked BEFORE config-based executors, allowing them to override
/// YAML-defined task configurations with rich Rust implementations.
#[async_trait::async_trait]
pub trait TypedTaskProvider: Send + Sync {
    /// Check if a typed task exists with the given name
    fn has_task(&self, name: &str) -> bool;

    /// Validate whether a task has already been completed (idempotency check)
    async fn validate(&self, name: &str) -> Result<bool, OrchestrationError>;

    /// Execute a typed task, returning a stream of JSON state updates
    ///
    /// Returns a tuple of:
    /// - A receiver for JSON state updates
    /// - A future that performs the JSON conversion (must be spawned)
    async fn execute(
        &self,
        name: &str,
        spawner: &dyn Spawner,
    ) -> Result<Receiver<JsonValue>, OrchestrationError>;
}

/// Central task orchestrator
///
/// The TaskManager orchestrates both config-based tasks (shell commands from YAML)
/// and typed tasks (Rust state machines). Typed tasks take precedence when both
/// exist for the same task name.
///
/// # Internal Mutability
///
/// - `task_configs`: RwLock for thread-safe access to task configurations
/// - `task_executions`: RwLock for tracking running/completed task state
/// - `typed_task_provider`: Optional provider set at construction time (immutable after)
pub struct TaskManager {
    /// Task executors by type (for config-based tasks)
    executors: HashMap<String, Arc<dyn TaskExecutor>>,

    /// Task configurations from YAML
    task_configs: Arc<RwLock<HashMap<String, crate::TaskConfig>>>,

    /// Currently running or completed tasks
    task_executions: Arc<RwLock<HashMap<String, TaskExecution>>>,

    /// Optional provider for typed tasks (state-machine implementations)
    ///
    /// When set, typed tasks are checked first before falling back to
    /// config-based executors.
    typed_task_provider: Option<Arc<dyn TypedTaskProvider>>,
}

impl TaskManager {
    /// Create a new task manager
    pub fn new() -> Self {
        Self {
            executors: HashMap::new(),
            task_configs: Arc::new(RwLock::new(HashMap::new())),
            task_executions: Arc::new(RwLock::new(HashMap::new())),
            typed_task_provider: None,
        }
    }

    /// Set the typed task provider
    ///
    /// When set, typed tasks are checked first before falling back to
    /// config-based executors. This allows Rust state-machine implementations
    /// to override YAML-defined shell commands.
    pub fn set_typed_task_provider(&mut self, provider: Arc<dyn TypedTaskProvider>) {
        self.typed_task_provider = Some(provider);
    }

    /// Register a task executor for config-based tasks
    pub fn register_executor(&mut self, name: String, executor: Arc<dyn TaskExecutor>) {
        self.executors.insert(name, executor);
    }

    /// Register a task configuration
    pub fn register_task(&self, name: String, config: crate::TaskConfig) {
        self.task_configs.write().unwrap().insert(name, config);
    }
    
    /// Find the appropriate executor for a task configuration
    fn find_executor(&self, config: &crate::TaskConfig) -> Result<Arc<dyn TaskExecutor>, OrchestrationError> {
        for executor in self.executors.values() {
            if executor.can_handle(config) {
                return Ok(executor.clone());
            }
        }
        
        Err(OrchestrationError::Config(format!(
            "No executor found for task target: {:?}",
            config.target
        )))
    }
    
    /// Execute a task
    ///
    /// Typed tasks (Rust state machines) take precedence over config-based tasks.
    /// If a typed task provider is set and has a task with this name, it will be
    /// executed. Otherwise, falls back to config-based execution via TaskExecutor.
    pub async fn execute_task(
        &self,
        name: &str,
        spawner: &dyn Spawner,
    ) -> Result<TaskExecution, OrchestrationError> {
        debug!("Executing task: {}", name);

        // Check if task has already been executed
        {
            let executions = self.task_executions.read().unwrap();
            if let Some(existing) = executions.get(name) {
                match &existing.status {
                    TaskStatus::Completed => {
                        debug!("Task {} already completed, skipping", name);
                        return Ok(existing.clone());
                    }
                    TaskStatus::Running => {
                        warn!("Task {} is already running", name);
                        return Ok(existing.clone());
                    }
                    _ => {
                        // Failed or NotStarted - we can retry
                    }
                }
            }
        }

        // Check for typed task first (takes precedence over config-based)
        if let Some(provider) = &self.typed_task_provider {
            if provider.has_task(name) {
                debug!("Found typed task implementation for: {}", name);
                return self.execute_typed_task(name, provider.clone(), spawner).await;
            }
        }

        // Fall back to config-based execution
        self.execute_config_task(name, spawner).await
    }

    /// Execute a typed task via the TypedTaskProvider
    async fn execute_typed_task(
        &self,
        name: &str,
        provider: Arc<dyn TypedTaskProvider>,
        spawner: &dyn Spawner,
    ) -> Result<TaskExecution, OrchestrationError> {
        // Check if task is already complete (idempotency check)
        if provider.validate(name).await? {
            debug!("Typed task {} is already complete (validated), skipping execution", name);
            let execution = TaskExecution {
                name: name.to_string(),
                status: TaskStatus::Completed,
                state_receiver: None,
                outputs: HashMap::new(),
            };

            self.task_executions
                .write()
                .unwrap()
                .insert(name.to_string(), execution.clone());

            return Ok(execution);
        }

        // Mark task as running
        {
            let mut executions = self.task_executions.write().unwrap();
            executions.insert(
                name.to_string(),
                TaskExecution {
                    name: name.to_string(),
                    status: TaskStatus::Running,
                    state_receiver: None,
                    outputs: HashMap::new(),
                },
            );
        }

        // Execute the typed task
        let state_receiver = match provider.execute(name, spawner).await {
            Ok(rx) => rx,
            Err(e) => {
                let mut executions = self.task_executions.write().unwrap();
                executions.insert(
                    name.to_string(),
                    TaskExecution {
                        name: name.to_string(),
                        status: TaskStatus::Failed(e.to_string()),
                        state_receiver: None,
                        outputs: HashMap::new(),
                    },
                );
                return Err(OrchestrationError::Other(e.to_string()));
            }
        };

        // Spawn monitoring task and return execution
        self.spawn_state_monitor(name, state_receiver.clone(), spawner);

        Ok(TaskExecution {
            name: name.to_string(),
            status: TaskStatus::Running,
            state_receiver: Some(state_receiver),
            outputs: HashMap::new(),
        })
    }

    /// Execute a config-based task via TaskExecutor
    async fn execute_config_task(
        &self,
        name: &str,
        spawner: &dyn Spawner,
    ) -> Result<TaskExecution, OrchestrationError> {
        // Get task configuration
        let config = {
            let configs = self.task_configs.read().unwrap();
            configs.get(name).cloned().ok_or_else(|| {
                OrchestrationError::Config(format!("Task configuration not found: {}", name))
            })?
        };

        // Find appropriate executor
        let executor = self.find_executor(&config)?;

        // Check if task is already complete (idempotency check)
        if executor.is_complete(name, &config).await? {
            debug!("Task {} is already complete (validated), skipping execution", name);
            let execution = TaskExecution {
                name: name.to_string(),
                status: TaskStatus::Completed,
                state_receiver: None,
                outputs: HashMap::new(),
            };

            self.task_executions
                .write()
                .unwrap()
                .insert(name.to_string(), execution.clone());

            return Ok(execution);
        }

        // Mark task as running
        {
            let mut executions = self.task_executions.write().unwrap();
            executions.insert(
                name.to_string(),
                TaskExecution {
                    name: name.to_string(),
                    status: TaskStatus::Running,
                    state_receiver: None,
                    outputs: HashMap::new(),
                },
            );
        }

        // Execute the task
        let state_receiver = match executor.execute(name, &config, spawner).await {
            Ok(rx) => rx,
            Err(e) => {
                let mut executions = self.task_executions.write().unwrap();
                executions.insert(
                    name.to_string(),
                    TaskExecution {
                        name: name.to_string(),
                        status: TaskStatus::Failed(e.to_string()),
                        state_receiver: None,
                        outputs: HashMap::new(),
                    },
                );
                return Err(e);
            }
        };

        // Spawn monitoring task
        self.spawn_state_monitor(name, state_receiver.clone(), spawner);

        Ok(TaskExecution {
            name: name.to_string(),
            status: TaskStatus::Running,
            state_receiver: Some(state_receiver),
            outputs: HashMap::new(),
        })
    }

    /// Spawn a task to monitor state updates and update task execution status
    fn spawn_state_monitor(
        &self,
        name: &str,
        state_receiver: Receiver<JsonValue>,
        spawner: &dyn Spawner,
    ) {
        let task_name = name.to_string();
        let executions = self.task_executions.clone();

        spawner.spawn_detached(Box::pin(async move {
            let mut last_state = None;

            while let Ok(state) = state_receiver.recv().await {
                debug!("Task {} state update: {:?}", task_name, state);
                last_state = Some(state.clone());

                // Check for completion or failure
                if let JsonValue::String(state_str) = &state {
                    if state_str == "Completed" {
                        let mut execs = executions.write().unwrap();
                        if let Some(exec) = execs.get_mut(&task_name) {
                            exec.status = TaskStatus::Completed;
                        }
                        break;
                    } else if state_str == "Failed" {
                        let mut execs = executions.write().unwrap();
                        if let Some(exec) = execs.get_mut(&task_name) {
                            exec.status = TaskStatus::Failed("Task reported failure".to_string());
                        }
                        break;
                    }
                }

                // Check for object state with status and outputs
                if let JsonValue::Object(obj) = &state {
                    if let Some(JsonValue::String(status)) = obj.get("status") {
                        if status == "Completed" || status == "completed" {
                            let mut execs = executions.write().unwrap();
                            if let Some(exec) = execs.get_mut(&task_name) {
                                exec.status = TaskStatus::Completed;
                                // Extract outputs if present
                                if let Some(JsonValue::Object(outputs)) = obj.get("outputs") {
                                    for (key, value) in outputs {
                                        exec.outputs.insert(key.clone(), value.clone());
                                    }
                                    debug!(
                                        "Task {} completed with {} outputs",
                                        task_name,
                                        exec.outputs.len()
                                    );
                                }
                            }
                            break;
                        } else if status == "Failed" || status == "failed" {
                            let error = obj
                                .get("error")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Task reported failure");
                            let mut execs = executions.write().unwrap();
                            if let Some(exec) = execs.get_mut(&task_name) {
                                exec.status = TaskStatus::Failed(error.to_string());
                            }
                            break;
                        }
                    }
                }
            }

            // If we didn't get a clear completion/failure, check the last state
            if let Some(final_state) = last_state {
                let mut execs = executions.write().unwrap();
                if let Some(exec) = execs.get_mut(&task_name) {
                    // Only update if still Running
                    if matches!(exec.status, TaskStatus::Running) {
                        // Try to determine from final state
                        if let JsonValue::String(state_str) = &final_state {
                            if state_str.contains("Complete") || state_str.contains("Success") {
                                exec.status = TaskStatus::Completed;
                            } else if state_str.contains("Fail") || state_str.contains("Error") {
                                exec.status =
                                    TaskStatus::Failed("Task ended in error state".to_string());
                            } else {
                                // Assume completion if channel closed without error
                                exec.status = TaskStatus::Completed;
                            }
                        } else if let JsonValue::Object(obj) = &final_state {
                            // Extract outputs from final state if present
                            if let Some(JsonValue::Object(outputs)) = obj.get("outputs") {
                                for (key, value) in outputs {
                                    exec.outputs.insert(key.clone(), value.clone());
                                }
                            }
                            exec.status = TaskStatus::Completed;
                        } else {
                            // Channel closed, assume completion
                            exec.status = TaskStatus::Completed;
                        }
                    }
                }
            }

            debug!("Task {} monitoring complete", task_name);
        }));
    }

    /// Wait for a task to complete
    pub async fn wait_for_completion(
        &self,
        name: &str,
    ) -> Result<TaskStatus, OrchestrationError> {
        // If we have a state receiver, wait for updates
        let execution = {
            let executions = self.task_executions.read().unwrap();
            executions.get(name).cloned()
        };
        
        if let Some(exec) = execution {
            if let Some(rx) = exec.state_receiver {
                // Consume all state updates
                while let Ok(_state) = rx.recv().await {
                    // State updates are being monitored by the spawned task
                }
            }
            
            // Get the final status
            let executions = self.task_executions.read().unwrap();
            if let Some(final_exec) = executions.get(name) {
                return Ok(final_exec.status.clone());
            }
        }
        
        Err(OrchestrationError::Config(format!("Task {} not found", name)))
    }
    
    /// Get the status of a task
    pub fn get_task_status(&self, name: &str) -> Option<TaskStatus> {
        let executions = self.task_executions.read().unwrap();
        executions.get(name).map(|e| e.status.clone())
    }
    
    /// Get all task executions
    pub fn get_all_executions(&self) -> HashMap<String, TaskExecution> {
        let executions = self.task_executions.read().unwrap();
        executions.clone()
    }

    /// Get outputs from a completed task
    ///
    /// Returns None if the task doesn't exist or hasn't completed.
    pub fn get_task_outputs(&self, name: &str) -> Option<HashMap<String, JsonValue>> {
        let executions = self.task_executions.read().unwrap();
        executions.get(name).and_then(|exec| {
            if matches!(exec.status, TaskStatus::Completed) {
                Some(exec.outputs.clone())
            } else {
                None
            }
        })
    }

    /// Get a specific output from a completed task
    ///
    /// Returns None if the task doesn't exist, hasn't completed, or doesn't have the output.
    pub fn get_task_output(&self, task_name: &str, output_key: &str) -> Option<JsonValue> {
        let executions = self.task_executions.read().unwrap();
        executions.get(task_name).and_then(|exec| {
            if matches!(exec.status, TaskStatus::Completed) {
                exec.outputs.get(output_key).cloned()
            } else {
                None
            }
        })
    }

    /// Get a task output as a string
    ///
    /// Convenience method for getting string outputs like contract addresses.
    pub fn get_task_output_str(&self, task_name: &str, output_key: &str) -> Option<String> {
        self.get_task_output(task_name, output_key)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
    }
}