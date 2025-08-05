//! Runtime state management for orchestration
//!
//! This module provides enhanced state tracking and querying capabilities
//! for the orchestration system, including deployment state, task execution
//! tracking, and state validation.

use crate::{Error, ServiceConfig, TaskConfig, config::ServiceStatus};
use chrono::{DateTime, Utc};
use service_registry::ServiceState as RegistryServiceState;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Deployment state tracking
#[derive(Debug, Clone)]
pub struct DeploymentState {
    /// Unique deployment ID
    pub id: Uuid,
    /// Stack name being deployed
    pub stack_name: String,
    /// When deployment started
    pub started_at: DateTime<Utc>,
    /// When deployment completed (if finished)
    pub completed_at: Option<DateTime<Utc>>,
    /// Overall deployment status
    pub status: DeploymentStatus,
    /// Service states within this deployment
    pub services: HashMap<String, ServiceDeploymentState>,
    /// Task states within this deployment
    pub tasks: HashMap<String, TaskExecutionState>,
    /// Deployment errors
    pub errors: Vec<DeploymentError>,
}

/// Overall deployment status
#[derive(Debug, Clone, PartialEq)]
pub enum DeploymentStatus {
    /// Deployment is in progress
    InProgress,
    /// Deployment completed successfully
    Completed,
    /// Deployment failed
    Failed,
    /// Deployment was cancelled
    Cancelled,
}

/// Service deployment state
#[derive(Debug, Clone)]
pub struct ServiceDeploymentState {
    /// Service name
    pub name: String,
    /// Current state
    pub state: ServiceState,
    /// When state last changed
    pub last_state_change: DateTime<Utc>,
    /// Service instance ID if running
    pub instance_id: Option<Uuid>,
    /// Error if failed
    pub error: Option<String>,
}

/// Enhanced service state with more granular tracking
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceState {
    /// Waiting for dependencies
    WaitingForDependencies,
    /// Service is starting
    Starting,
    /// Service is running setup
    RunningSetup,
    /// Service is running
    Running,
    /// Service is unhealthy
    Unhealthy(String),
    /// Service is stopping
    Stopping,
    /// Service is stopped
    Stopped,
    /// Service failed to start
    Failed(String),
}

impl From<ServiceState> for RegistryServiceState {
    fn from(state: ServiceState) -> Self {
        match state {
            ServiceState::WaitingForDependencies => RegistryServiceState::Registered,
            ServiceState::Starting => RegistryServiceState::Starting,
            ServiceState::RunningSetup => RegistryServiceState::Starting,
            ServiceState::Running => RegistryServiceState::Running,
            ServiceState::Unhealthy(_) => RegistryServiceState::Failed,
            ServiceState::Stopping => RegistryServiceState::Stopping,
            ServiceState::Stopped => RegistryServiceState::Stopped,
            ServiceState::Failed(_) => RegistryServiceState::Failed,
        }
    }
}

/// Task execution state
#[derive(Debug, Clone)]
pub struct TaskExecutionState {
    /// Task name
    pub name: String,
    /// Current state
    pub state: TaskState,
    /// When task started
    pub started_at: Option<DateTime<Utc>>,
    /// When task completed
    pub completed_at: Option<DateTime<Utc>>,
    /// Exit code if completed
    pub exit_code: Option<i32>,
    /// Error if failed
    pub error: Option<String>,
}

/// Task execution state
#[derive(Debug, Clone, PartialEq)]
pub enum TaskState {
    /// Waiting for dependencies
    WaitingForDependencies,
    /// Task is queued
    Queued,
    /// Task is running
    Running,
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed(String),
    /// Task was skipped
    Skipped(String),
}

/// Deployment error information
#[derive(Debug, Clone)]
pub struct DeploymentError {
    /// When error occurred
    pub timestamp: DateTime<Utc>,
    /// Component that failed (service/task name)
    pub component: String,
    /// Error message
    pub message: String,
}

/// State manager for tracking runtime state
pub struct StateManager {
    /// Current deployment state
    current_deployment: Arc<RwLock<Option<DeploymentState>>>,
    /// Historical deployments (last N)
    deployment_history: Arc<RwLock<Vec<DeploymentState>>>,
    /// Maximum deployment history size
    max_history_size: usize,
}

impl StateManager {
    /// Create a new state manager
    pub fn new() -> Self {
        Self {
            current_deployment: Arc::new(RwLock::new(None)),
            deployment_history: Arc::new(RwLock::new(Vec::new())),
            max_history_size: 10,
        }
    }

    /// Start a new deployment
    pub fn start_deployment(&self, stack_name: String) -> Uuid {
        let deployment = DeploymentState {
            id: Uuid::new_v4(),
            stack_name,
            started_at: Utc::now(),
            completed_at: None,
            status: DeploymentStatus::InProgress,
            services: HashMap::new(),
            tasks: HashMap::new(),
            errors: Vec::new(),
        };

        let deployment_id = deployment.id;
        *self.current_deployment.write().unwrap() = Some(deployment);
        deployment_id
    }

    /// Update service state
    pub fn update_service_state(
        &self,
        service_name: &str,
        state: ServiceState,
        instance_id: Option<Uuid>,
    ) -> Result<(), Error> {
        let mut current = self.current_deployment.write().unwrap();
        let deployment = current
            .as_mut()
            .ok_or_else(|| Error::Other("No active deployment".to_string()))?;

        let service_state = deployment
            .services
            .entry(service_name.to_string())
            .or_insert_with(|| ServiceDeploymentState {
                name: service_name.to_string(),
                state: ServiceState::WaitingForDependencies,
                last_state_change: Utc::now(),
                instance_id: None,
                error: None,
            });

        // Update error field based on state
        match &state {
            ServiceState::Failed(err) | ServiceState::Unhealthy(err) => {
                service_state.error = Some(err.clone());
            }
            _ => {
                service_state.error = None;
            }
        }

        service_state.state = state;
        service_state.last_state_change = Utc::now();
        if let Some(id) = instance_id {
            service_state.instance_id = Some(id);
        }

        Ok(())
    }

    /// Update task state
    pub fn update_task_state(&self, task_name: &str, state: TaskState) -> Result<(), Error> {
        let mut current = self.current_deployment.write().unwrap();
        let deployment = current
            .as_mut()
            .ok_or_else(|| Error::Other("No active deployment".to_string()))?;

        let task_state = deployment
            .tasks
            .entry(task_name.to_string())
            .or_insert_with(|| TaskExecutionState {
                name: task_name.to_string(),
                state: TaskState::WaitingForDependencies,
                started_at: None,
                completed_at: None,
                exit_code: None,
                error: None,
            });

        // Update timestamps and error based on state transition
        match &state {
            TaskState::Running => {
                if task_state.started_at.is_none() {
                    task_state.started_at = Some(Utc::now());
                }
            }
            TaskState::Completed => {
                task_state.completed_at = Some(Utc::now());
                task_state.exit_code = Some(0);
            }
            TaskState::Failed(err) => {
                task_state.completed_at = Some(Utc::now());
                task_state.error = Some(err.clone());
            }
            TaskState::Skipped(reason) => {
                task_state.error = Some(reason.clone());
            }
            _ => {}
        }

        task_state.state = state;
        Ok(())
    }

    /// Record a deployment error
    pub fn record_error(&self, component: &str, message: String) -> Result<(), Error> {
        let mut current = self.current_deployment.write().unwrap();
        let deployment = current
            .as_mut()
            .ok_or_else(|| Error::Other("No active deployment".to_string()))?;

        deployment.errors.push(DeploymentError {
            timestamp: Utc::now(),
            component: component.to_string(),
            message,
        });

        Ok(())
    }

    /// Complete the current deployment
    pub fn complete_deployment(&self, status: DeploymentStatus) -> Result<(), Error> {
        let mut current = self.current_deployment.write().unwrap();
        let mut deployment = current
            .take()
            .ok_or_else(|| Error::Other("No active deployment".to_string()))?;

        deployment.completed_at = Some(Utc::now());
        deployment.status = status;

        // Add to history
        let mut history = self.deployment_history.write().unwrap();
        history.push(deployment);

        // Trim history if needed
        if history.len() > self.max_history_size {
            history.remove(0);
        }

        Ok(())
    }

    /// Get current deployment state
    pub fn get_current_deployment(&self) -> Option<DeploymentState> {
        self.current_deployment.read().unwrap().clone()
    }

    /// Get deployment history
    pub fn get_deployment_history(&self) -> Vec<DeploymentState> {
        self.deployment_history.read().unwrap().clone()
    }

    /// Query service states
    pub fn query_services(&self, filter: ServiceStateFilter) -> Vec<ServiceDeploymentState> {
        let current = self.current_deployment.read().unwrap();
        if let Some(deployment) = current.as_ref() {
            deployment
                .services
                .values()
                .filter(|s| filter.matches(&s.state))
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Query task states
    pub fn query_tasks(&self, filter: TaskStateFilter) -> Vec<TaskExecutionState> {
        let current = self.current_deployment.read().unwrap();
        if let Some(deployment) = current.as_ref() {
            deployment
                .tasks
                .values()
                .filter(|t| filter.matches(&t.state))
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Check if all services are healthy
    pub fn all_services_healthy(&self) -> bool {
        let current = self.current_deployment.read().unwrap();
        if let Some(deployment) = current.as_ref() {
            deployment
                .services
                .values()
                .all(|s| matches!(s.state, ServiceState::Running))
        } else {
            true
        }
    }

    /// Get deployment summary
    pub fn get_deployment_summary(&self) -> Option<DeploymentSummary> {
        let current = self.current_deployment.read().unwrap();
        current.as_ref().map(|deployment| {
            let total_services = deployment.services.len();
            let running_services = deployment
                .services
                .values()
                .filter(|s| matches!(s.state, ServiceState::Running))
                .count();
            let failed_services = deployment
                .services
                .values()
                .filter(|s| matches!(s.state, ServiceState::Failed(_)))
                .count();

            let total_tasks = deployment.tasks.len();
            let completed_tasks = deployment
                .tasks
                .values()
                .filter(|t| matches!(t.state, TaskState::Completed))
                .count();
            let failed_tasks = deployment
                .tasks
                .values()
                .filter(|t| matches!(t.state, TaskState::Failed(_)))
                .count();

            DeploymentSummary {
                deployment_id: deployment.id,
                stack_name: deployment.stack_name.clone(),
                status: deployment.status.clone(),
                started_at: deployment.started_at,
                duration: deployment
                    .completed_at
                    .map(|end| end - deployment.started_at)
                    .unwrap_or_else(|| Utc::now() - deployment.started_at),
                total_services,
                running_services,
                failed_services,
                total_tasks,
                completed_tasks,
                failed_tasks,
                error_count: deployment.errors.len(),
            }
        })
    }
}

/// Service state filter for queries
#[derive(Debug, Clone)]
pub enum ServiceStateFilter {
    /// Match any state
    Any,
    /// Match specific state
    State(ServiceState),
    /// Match running or healthy
    Healthy,
    /// Match any failed state
    Failed,
    /// Match specific states
    States(Vec<ServiceState>),
}

impl ServiceStateFilter {
    fn matches(&self, state: &ServiceState) -> bool {
        match self {
            ServiceStateFilter::Any => true,
            ServiceStateFilter::State(s) => state == s,
            ServiceStateFilter::Healthy => matches!(state, ServiceState::Running),
            ServiceStateFilter::Failed => {
                matches!(state, ServiceState::Failed(_) | ServiceState::Unhealthy(_))
            }
            ServiceStateFilter::States(states) => states.contains(state),
        }
    }
}

/// Task state filter for queries
#[derive(Debug, Clone)]
pub enum TaskStateFilter {
    /// Match any state
    Any,
    /// Match specific state
    State(TaskState),
    /// Match completed tasks
    Completed,
    /// Match failed tasks
    Failed,
    /// Match running tasks
    Running,
}

impl TaskStateFilter {
    fn matches(&self, state: &TaskState) -> bool {
        match self {
            TaskStateFilter::Any => true,
            TaskStateFilter::State(s) => state == s,
            TaskStateFilter::Completed => matches!(state, TaskState::Completed),
            TaskStateFilter::Failed => matches!(state, TaskState::Failed(_)),
            TaskStateFilter::Running => matches!(state, TaskState::Running),
        }
    }
}

/// Deployment summary information
#[derive(Debug, Clone)]
pub struct DeploymentSummary {
    pub deployment_id: Uuid,
    pub stack_name: String,
    pub status: DeploymentStatus,
    pub started_at: DateTime<Utc>,
    pub duration: chrono::Duration,
    pub total_services: usize,
    pub running_services: usize,
    pub failed_services: usize,
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub failed_tasks: usize,
    pub error_count: usize,
}

impl Default for StateManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_manager_deployment_lifecycle() {
        let state_manager = StateManager::new();

        // Start deployment
        let deployment_id = state_manager.start_deployment("test-stack".to_string());
        assert!(state_manager.get_current_deployment().is_some());

        // Update service state
        state_manager
            .update_service_state("service1", ServiceState::Starting, None)
            .unwrap();
        state_manager
            .update_service_state("service1", ServiceState::Running, Some(Uuid::new_v4()))
            .unwrap();

        // Update task state
        state_manager
            .update_task_state("task1", TaskState::Running)
            .unwrap();
        state_manager
            .update_task_state("task1", TaskState::Completed)
            .unwrap();

        // Complete deployment
        state_manager
            .complete_deployment(DeploymentStatus::Completed)
            .unwrap();
        assert!(state_manager.get_current_deployment().is_none());
        assert_eq!(state_manager.get_deployment_history().len(), 1);
    }

    #[test]
    fn test_state_queries() {
        let state_manager = StateManager::new();
        state_manager.start_deployment("test-stack".to_string());

        // Add services in different states
        state_manager
            .update_service_state("service1", ServiceState::Running, Some(Uuid::new_v4()))
            .unwrap();
        state_manager
            .update_service_state("service2", ServiceState::Failed("error".to_string()), None)
            .unwrap();
        state_manager
            .update_service_state("service3", ServiceState::Starting, None)
            .unwrap();

        // Query healthy services
        let healthy = state_manager.query_services(ServiceStateFilter::Healthy);
        assert_eq!(healthy.len(), 1);
        assert_eq!(healthy[0].name, "service1");

        // Query failed services
        let failed = state_manager.query_services(ServiceStateFilter::Failed);
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].name, "service2");

        // Check all healthy
        assert!(!state_manager.all_services_healthy());
    }

    #[test]
    fn test_deployment_summary() {
        let state_manager = StateManager::new();
        state_manager.start_deployment("test-stack".to_string());

        // Add services and tasks
        state_manager
            .update_service_state("service1", ServiceState::Running, Some(Uuid::new_v4()))
            .unwrap();
        state_manager
            .update_service_state("service2", ServiceState::Failed("error".to_string()), None)
            .unwrap();

        state_manager
            .update_task_state("task1", TaskState::Completed)
            .unwrap();
        state_manager
            .update_task_state("task2", TaskState::Failed("error".to_string()))
            .unwrap();

        state_manager
            .record_error("service2", "Failed to start".to_string())
            .unwrap();

        let summary = state_manager.get_deployment_summary().unwrap();
        assert_eq!(summary.total_services, 2);
        assert_eq!(summary.running_services, 1);
        assert_eq!(summary.failed_services, 1);
        assert_eq!(summary.total_tasks, 2);
        assert_eq!(summary.completed_tasks, 1);
        assert_eq!(summary.failed_tasks, 1);
        assert_eq!(summary.error_count, 1);
    }
}
