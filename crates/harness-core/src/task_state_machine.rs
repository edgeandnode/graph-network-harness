//! State machine infrastructure for robust task orchestration
//!
//! This module provides generic state machine traits and implementations
//! for complex deployment tasks that require multiple steps with verification.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
// Removed unused statig prelude import
use std::collections::HashMap;
use std::fmt::Debug;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::{Error, Result};

/// Common states for deployment tasks
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaskExecutionState {
    /// Initial state - task has not started
    Idle,
    /// Checking if the task needs to be executed (idempotency check)
    CheckingPrerequisites,
    /// Prerequisites met, preparing to execute
    Preparing,
    /// Actively executing the main task
    Executing,
    /// Verifying the task completed successfully
    Verifying,
    /// Task completed successfully
    Completed,
    /// Task failed with an error
    Failed(String),
    /// Rolling back changes after failure
    RollingBack,
}

/// Events that trigger state transitions
#[derive(Debug, Clone)]
pub enum TaskEvent {
    /// Start the task
    Start,
    /// Prerequisites have been checked
    PrerequisitesChecked {
        /// Whether the task was already complete when checked
        already_complete: bool,
    },
    /// Preparation is complete
    PreparedSuccessfully,
    /// Execution completed
    ExecutionCompleted,
    /// Verification passed
    VerificationPassed,
    /// Verification failed
    VerificationFailed(String),
    /// An error occurred
    ErrorOccurred(String),
    /// Retry the task
    Retry,
    /// Rollback completed
    RollbackCompleted,
}

/// Context for task execution with progress tracking
#[derive(Debug, Clone)]
pub struct TaskContext {
    /// Task name
    pub name: String,
    /// Current progress percentage (0-100)
    pub progress: u8,
    /// Status message
    pub status_message: String,
    /// Retry count
    pub retry_count: u32,
    /// Maximum retries allowed
    pub max_retries: u32,
    /// Task-specific data
    pub data: HashMap<String, serde_json::Value>,
    /// Start time of current execution
    pub start_time: Option<Instant>,
}

impl TaskContext {
    /// Create a new task context
    pub fn new(name: String, max_retries: u32) -> Self {
        Self {
            name,
            progress: 0,
            status_message: "Initializing".to_string(),
            retry_count: 0,
            max_retries,
            data: HashMap::new(),
            start_time: None,
        }
    }

    /// Update progress
    pub fn set_progress(&mut self, progress: u8, message: impl Into<String>) {
        self.progress = progress.min(100);
        self.status_message = message.into();
        debug!(
            task = %self.name,
            progress = self.progress,
            status = %self.status_message,
            "Task progress update"
        );
    }

    /// Check if retries are exhausted
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }

    /// Increment retry count
    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
        info!(
            task = %self.name,
            retry = self.retry_count,
            max = self.max_retries,
            "Retrying task"
        );
    }

    /// Get elapsed time since start
    pub fn elapsed(&self) -> Option<Duration> {
        self.start_time.map(|start| start.elapsed())
    }
}

/// Trait for task-specific state machine implementations
#[async_trait]
pub trait TaskStateMachine: Send + Sync {
    /// Check if prerequisites are met
    async fn check_prerequisites(&mut self, context: &mut TaskContext) -> Result<bool>;

    /// Prepare for execution
    async fn prepare(&mut self, context: &mut TaskContext) -> Result<()>;

    /// Execute the main task
    async fn execute(&mut self, context: &mut TaskContext) -> Result<()>;

    /// Verify the task completed successfully
    async fn verify(&mut self, context: &mut TaskContext) -> Result<()>;

    /// Rollback changes on failure
    async fn rollback(&mut self, context: &mut TaskContext) -> Result<()>;

    /// Get current state for persistence
    fn get_state(&self) -> TaskExecutionState;

    /// Restore from persisted state
    fn set_state(&mut self, state: TaskExecutionState);
}

/// Generic state machine wrapper for tasks
pub struct TaskStateMachineWrapper<T: TaskStateMachine> {
    /// The inner task implementation
    inner: T,
    /// Execution context
    context: TaskContext,
}

impl<T: TaskStateMachine> TaskStateMachineWrapper<T> {
    /// Create a new state machine wrapper
    pub fn new(task: T, name: String, max_retries: u32) -> Self {
        Self {
            inner: task,
            context: TaskContext::new(name, max_retries),
        }
    }

    /// Get the current context
    pub fn context(&self) -> &TaskContext {
        &self.context
    }

    /// Get mutable context
    pub fn context_mut(&mut self) -> &mut TaskContext {
        &mut self.context
    }

    /// Process an event and transition states
    pub async fn process_event(&mut self, event: TaskEvent) -> Result<TaskExecutionState> {
        let current_state = self.inner.get_state();
        info!(
            task = %self.context.name,
            ?current_state,
            ?event,
            "Processing task event"
        );

        let new_state = match (current_state.clone(), event) {
            // Starting from idle
            (TaskExecutionState::Idle, TaskEvent::Start) => {
                self.context.start_time = Some(Instant::now());
                self.context.set_progress(5, "Checking prerequisites");
                TaskExecutionState::CheckingPrerequisites
            }

            // After checking prerequisites
            (
                TaskExecutionState::CheckingPrerequisites,
                TaskEvent::PrerequisitesChecked { already_complete },
            ) => {
                if already_complete {
                    self.context.set_progress(100, "Already completed");
                    TaskExecutionState::Completed
                } else {
                    self.context.set_progress(10, "Preparing");
                    TaskExecutionState::Preparing
                }
            }

            // After preparation
            (TaskExecutionState::Preparing, TaskEvent::PreparedSuccessfully) => {
                self.context.set_progress(20, "Executing");
                TaskExecutionState::Executing
            }

            // After execution
            (TaskExecutionState::Executing, TaskEvent::ExecutionCompleted) => {
                self.context.set_progress(80, "Verifying");
                TaskExecutionState::Verifying
            }

            // After verification
            (TaskExecutionState::Verifying, TaskEvent::VerificationPassed) => {
                self.context.set_progress(100, "Completed");
                TaskExecutionState::Completed
            }

            (TaskExecutionState::Verifying, TaskEvent::VerificationFailed(reason)) => {
                self.context
                    .set_progress(85, format!("Verification failed: {reason}"));
                if self.context.can_retry() {
                    self.context.increment_retry();
                    TaskExecutionState::Preparing // Retry from preparation
                } else {
                    TaskExecutionState::Failed(reason)
                }
            }

            // Error handling from any state
            (_, TaskEvent::ErrorOccurred(error)) => {
                error!(
                    task = %self.context.name,
                    %error,
                    retry_count = self.context.retry_count,
                    "Task error occurred"
                );

                if self.context.can_retry() {
                    self.context.increment_retry();
                    TaskExecutionState::Preparing // Retry from preparation
                } else {
                    self.context.set_progress(0, format!("Failed: {error}"));
                    TaskExecutionState::RollingBack
                }
            }

            // After rollback
            (TaskExecutionState::RollingBack, TaskEvent::RollbackCompleted) => {
                TaskExecutionState::Failed("Rolled back after failure".to_string())
            }

            // Retry from failed state
            (TaskExecutionState::Failed(_), TaskEvent::Retry) => {
                if self.context.can_retry() {
                    self.context.increment_retry();
                    self.context.start_time = Some(Instant::now());
                    TaskExecutionState::CheckingPrerequisites
                } else {
                    current_state // Stay in failed state
                }
            }

            // Invalid transitions
            (state, event) => {
                warn!(
                    task = %self.context.name,
                    ?state,
                    ?event,
                    "Invalid state transition attempted"
                );
                state // No transition
            }
        };

        self.inner.set_state(new_state.clone());
        Ok(new_state)
    }

    /// Run the state machine to completion
    pub async fn run(&mut self) -> Result<()> {
        // Start the task
        self.process_event(TaskEvent::Start).await?;

        loop {
            let state = self.inner.get_state();

            match state {
                TaskExecutionState::CheckingPrerequisites => {
                    match self.inner.check_prerequisites(&mut self.context).await {
                        Ok(already_complete) => {
                            self.process_event(TaskEvent::PrerequisitesChecked {
                                already_complete,
                            })
                            .await?;
                        }
                        Err(e) => {
                            self.process_event(TaskEvent::ErrorOccurred(e.to_string()))
                                .await?;
                        }
                    }
                }

                TaskExecutionState::Preparing => {
                    match self.inner.prepare(&mut self.context).await {
                        Ok(()) => {
                            self.process_event(TaskEvent::PreparedSuccessfully).await?;
                        }
                        Err(e) => {
                            self.process_event(TaskEvent::ErrorOccurred(e.to_string()))
                                .await?;
                        }
                    }
                }

                TaskExecutionState::Executing => {
                    match self.inner.execute(&mut self.context).await {
                        Ok(()) => {
                            self.process_event(TaskEvent::ExecutionCompleted).await?;
                        }
                        Err(e) => {
                            self.process_event(TaskEvent::ErrorOccurred(e.to_string()))
                                .await?;
                        }
                    }
                }

                TaskExecutionState::Verifying => match self.inner.verify(&mut self.context).await {
                    Ok(()) => {
                        self.process_event(TaskEvent::VerificationPassed).await?;
                    }
                    Err(e) => {
                        self.process_event(TaskEvent::VerificationFailed(e.to_string()))
                            .await?;
                    }
                },

                TaskExecutionState::RollingBack => {
                    match self.inner.rollback(&mut self.context).await {
                        Ok(()) => {
                            self.process_event(TaskEvent::RollbackCompleted).await?;
                        }
                        Err(e) => {
                            error!(
                                task = %self.context.name,
                                error = %e,
                                "Rollback failed"
                            );
                            // Force transition to failed state even if rollback fails
                            self.inner.set_state(TaskExecutionState::Failed(format!(
                                "Rollback failed: {e}"
                            )));
                        }
                    }
                }

                TaskExecutionState::Completed => {
                    info!(
                        task = %self.context.name,
                        elapsed = ?self.context.elapsed(),
                        "Task completed successfully"
                    );
                    return Ok(());
                }

                TaskExecutionState::Failed(ref reason) => {
                    error!(
                        task = %self.context.name,
                        reason = %reason,
                        elapsed = ?self.context.elapsed(),
                        retries = self.context.retry_count,
                        "Task failed"
                    );
                    return Err(Error::daemon(format!("Task failed: {reason}")));
                }

                TaskExecutionState::Idle => {
                    // Should not reach here after starting
                    return Err(Error::daemon("Task in unexpected Idle state"));
                }
            }
        }
    }
}

/// Progress update for external monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgress {
    /// Task name
    pub name: String,
    /// Current state
    pub state: TaskExecutionState,
    /// Progress percentage
    pub progress: u8,
    /// Status message
    pub message: String,
    /// Retry count
    pub retry_count: u32,
    /// Elapsed time in seconds
    pub elapsed_secs: Option<u64>,
}

impl From<&TaskContext> for TaskProgress {
    fn from(context: &TaskContext) -> Self {
        Self {
            name: context.name.clone(),
            state: TaskExecutionState::Idle, // Will be updated by wrapper
            progress: context.progress,
            message: context.status_message.clone(),
            retry_count: context.retry_count,
            elapsed_secs: context.elapsed().map(|d| d.as_secs()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test implementation of a simple task
    struct TestTask {
        state: TaskExecutionState,
        should_fail: bool,
        already_complete: bool,
    }

    impl TestTask {
        fn new(should_fail: bool, already_complete: bool) -> Self {
            Self {
                state: TaskExecutionState::Idle,
                should_fail,
                already_complete,
            }
        }
    }

    #[async_trait]
    impl TaskStateMachine for TestTask {
        async fn check_prerequisites(&mut self, context: &mut TaskContext) -> Result<bool> {
            context.set_progress(5, "Checking test prerequisites");
            Ok(self.already_complete)
        }

        async fn prepare(&mut self, context: &mut TaskContext) -> Result<()> {
            context.set_progress(15, "Preparing test task");
            if self.should_fail && context.retry_count == 0 {
                return Err(Error::daemon("Preparation failed"));
            }
            Ok(())
        }

        async fn execute(&mut self, context: &mut TaskContext) -> Result<()> {
            context.set_progress(50, "Executing test task");
            context
                .data
                .insert("test_key".to_string(), serde_json::json!("test_value"));
            Ok(())
        }

        async fn verify(&mut self, context: &mut TaskContext) -> Result<()> {
            context.set_progress(90, "Verifying test task");
            if context.data.get("test_key").is_some() {
                Ok(())
            } else {
                Err(Error::daemon("Verification failed: missing data"))
            }
        }

        async fn rollback(&mut self, _context: &mut TaskContext) -> Result<()> {
            // Clean up test data
            Ok(())
        }

        fn get_state(&self) -> TaskExecutionState {
            self.state.clone()
        }

        fn set_state(&mut self, state: TaskExecutionState) {
            self.state = state;
        }
    }

    #[smol_potat::test]
    async fn test_successful_task_execution() {
        let task = TestTask::new(false, false);
        let mut wrapper = TaskStateMachineWrapper::new(task, "test_task".to_string(), 3);

        wrapper.run().await.unwrap();

        assert_eq!(wrapper.inner.get_state(), TaskExecutionState::Completed);
        assert_eq!(wrapper.context.progress, 100);
        assert!(wrapper.context.data.contains_key("test_key"));
    }

    #[smol_potat::test]
    async fn test_task_already_complete() {
        let task = TestTask::new(false, true);
        let mut wrapper = TaskStateMachineWrapper::new(task, "test_task".to_string(), 3);

        wrapper.run().await.unwrap();

        assert_eq!(wrapper.inner.get_state(), TaskExecutionState::Completed);
        assert_eq!(wrapper.context.retry_count, 0);
    }

    #[smol_potat::test]
    async fn test_task_retry_on_failure() {
        let task = TestTask::new(true, false);
        let mut wrapper = TaskStateMachineWrapper::new(task, "test_task".to_string(), 3);

        // Should succeed on retry
        wrapper.run().await.unwrap();

        assert_eq!(wrapper.inner.get_state(), TaskExecutionState::Completed);
        assert_eq!(wrapper.context.retry_count, 1);
    }

    #[smol_potat::test]
    async fn test_task_exhausted_retries() {
        // Create a task that always fails
        struct AlwaysFailTask {
            state: TaskExecutionState,
        }

        #[async_trait]
        impl TaskStateMachine for AlwaysFailTask {
            async fn check_prerequisites(&mut self, _context: &mut TaskContext) -> Result<bool> {
                Ok(false)
            }

            async fn prepare(&mut self, _context: &mut TaskContext) -> Result<()> {
                Err(Error::daemon("Always fails"))
            }

            async fn execute(&mut self, _context: &mut TaskContext) -> Result<()> {
                unreachable!()
            }

            async fn verify(&mut self, _context: &mut TaskContext) -> Result<()> {
                unreachable!()
            }

            async fn rollback(&mut self, _context: &mut TaskContext) -> Result<()> {
                Ok(())
            }

            fn get_state(&self) -> TaskExecutionState {
                self.state.clone()
            }

            fn set_state(&mut self, state: TaskExecutionState) {
                self.state = state;
            }
        }

        let task = AlwaysFailTask {
            state: TaskExecutionState::Idle,
        };
        let mut wrapper = TaskStateMachineWrapper::new(task, "failing_task".to_string(), 2);

        let result = wrapper.run().await;
        assert!(result.is_err());
        assert!(matches!(
            wrapper.inner.get_state(),
            TaskExecutionState::Failed(_)
        ));
        assert_eq!(wrapper.context.retry_count, 2);
    }

    #[test]
    fn test_task_context() {
        let mut context = TaskContext::new("test".to_string(), 5);

        assert_eq!(context.progress, 0);
        assert!(context.can_retry());

        context.set_progress(50, "Half way");
        assert_eq!(context.progress, 50);
        assert_eq!(context.status_message, "Half way");

        // Test retry logic
        for _ in 0..5 {
            context.increment_retry();
        }
        assert!(!context.can_retry());
        assert_eq!(context.retry_count, 5);

        // Test progress clamping
        context.set_progress(150, "Over 100");
        assert_eq!(context.progress, 100);
    }

    #[test]
    fn test_task_progress_conversion() {
        let mut context = TaskContext::new("test".to_string(), 3);
        context.set_progress(75, "Three quarters done");
        context.retry_count = 1;

        let progress: TaskProgress = (&context).into();
        assert_eq!(progress.name, "test");
        assert_eq!(progress.progress, 75);
        assert_eq!(progress.message, "Three quarters done");
        assert_eq!(progress.retry_count, 1);
    }
}
