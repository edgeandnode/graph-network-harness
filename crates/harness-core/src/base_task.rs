//! Base task implementation for one-time operations
//!
//! This module provides a base task that represents a one-time operation
//! that streams state changes. Tasks can access services and other resources
//! through their context.

use async_channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::base_service::BaseService;
use crate::Error;
use std::result::Result;

/// Base task states that track progress
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BaseTaskState {
    /// Task has not started
    Idle,
    /// Task is running with progress
    Running {
        /// Progress percentage (0-100)
        progress: f32,
        /// Current status message
        message: String,
    },
    /// Task completed successfully
    Completed,
    /// Task failed with error
    Failed(String),
}

/// Context provided to tasks for accessing services and configuration
#[derive(Clone)]
pub struct TaskContext {
    /// Available services mapped by name
    services: Arc<HashMap<String, Arc<BaseService>>>,
    /// Task configuration
    config: HashMap<String, serde_json::Value>,
    /// Shared state between tasks
    shared_state: Arc<HashMap<String, serde_json::Value>>,
}

impl TaskContext {
    /// Create a new task context
    pub fn new(
        services: HashMap<String, Arc<BaseService>>,
        config: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            services: Arc::new(services),
            config,
            shared_state: Arc::new(HashMap::new()),
        }
    }
    
    /// Get a service by name
    pub fn get_service(&self, name: &str) -> Result<Arc<BaseService>, Error> {
        self.services
            .get(name)
            .cloned()
            .ok_or_else(|| Error::service_not_found(name))
    }
    
    /// Get configuration value
    pub fn get_config<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Result<T, Error> {
        self.config
            .get(key)
            .ok_or_else(|| Error::daemon(format!("Config key '{}' not found", key)))
            .and_then(|v| serde_json::from_value(v.clone())
                .map_err(|e| Error::daemon(format!("Failed to deserialize config: {}", e))))
    }
    
    /// Get shared state value
    pub fn get_state<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Result<T, Error> {
        self.shared_state
            .get(key)
            .ok_or_else(|| Error::daemon(format!("State key '{}' not found", key)))
            .and_then(|v| serde_json::from_value(v.clone())
                .map_err(|e| Error::daemon(format!("Failed to deserialize state: {}", e))))
    }
}

/// Base task that manages state streaming
pub struct BaseTask {
    /// Task name
    name: String,
    /// Stream of state changes
    state_stream: Receiver<BaseTaskState>,
    /// Sender for state updates (used by task implementation)
    state_sender: Sender<BaseTaskState>,
    /// Task context for accessing resources
    context: TaskContext,
    /// Current state
    current_state: BaseTaskState,
}

impl BaseTask {
    /// Create a new base task
    pub fn new(name: String, context: TaskContext) -> Self {
        let (tx, rx) = async_channel::unbounded();
        
        Self {
            name,
            state_stream: rx,
            state_sender: tx,
            context,
            current_state: BaseTaskState::Idle,
        }
    }
    
    /// Get task name
    pub fn name(&self) -> &str {
        &self.name
    }
    
    /// Get state update stream
    pub fn states(&self) -> &Receiver<BaseTaskState> {
        &self.state_stream
    }
    
    /// Get current state
    pub fn current_state(&self) -> &BaseTaskState {
        &self.current_state
    }
    
    /// Update task state
    pub async fn update_state(&mut self, state: BaseTaskState) -> Result<(), Error> {
        self.current_state = state.clone();
        self.state_sender
            .send(state)
            .await
            .map_err(|e| Error::daemon(format!("Failed to send state update: {}", e)))
    }
    
    /// Set progress for running state
    pub async fn set_progress(&mut self, progress: f32, message: impl Into<String>) -> Result<(), Error> {
        self.update_state(BaseTaskState::Running {
            progress,
            message: message.into(),
        }).await
    }
    
    /// Mark task as completed
    pub async fn complete(&mut self) -> Result<(), Error> {
        self.update_state(BaseTaskState::Completed).await
    }
    
    /// Mark task as failed
    pub async fn fail(&mut self, reason: impl Into<String>) -> Result<(), Error> {
        self.update_state(BaseTaskState::Failed(reason.into())).await
    }
    
    /// Get task context
    pub fn context(&self) -> &TaskContext {
        &self.context
    }
    
    /// Get a service from context
    pub fn get_service(&self, name: &str) -> Result<Arc<BaseService>, Error> {
        self.context.get_service(name)
    }
    
    /// Check if task is complete
    pub fn is_complete(&self) -> bool {
        matches!(self.current_state, BaseTaskState::Completed)
    }
    
    /// Check if task failed
    pub fn is_failed(&self) -> bool {
        matches!(self.current_state, BaseTaskState::Failed(_))
    }
    
    /// Check if task is running
    pub fn is_running(&self) -> bool {
        matches!(self.current_state, BaseTaskState::Running { .. })
    }
    
    /// Get progress if running
    pub fn progress(&self) -> Option<f32> {
        match &self.current_state {
            BaseTaskState::Running { progress, .. } => Some(*progress),
            _ => None,
        }
    }
}

/// Helper for building task contexts
pub struct TaskContextBuilder {
    services: HashMap<String, Arc<BaseService>>,
    config: HashMap<String, serde_json::Value>,
}

impl TaskContextBuilder {
    /// Create a new task context builder
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
            config: HashMap::new(),
        }
    }
    
    /// Add a service
    pub fn with_service(mut self, name: impl Into<String>, service: Arc<BaseService>) -> Self {
        self.services.insert(name.into(), service);
        self
    }
    
    /// Add configuration
    pub fn with_config(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.config.insert(key.into(), value);
        self
    }
    
    /// Build the context
    pub fn build(self) -> TaskContext {
        TaskContext::new(self.services, self.config)
    }
}

impl Default for TaskContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}