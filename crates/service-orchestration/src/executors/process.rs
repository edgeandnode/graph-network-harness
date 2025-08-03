//! Process executor for local service execution.

use super::{EventStream, RunningService, ServiceExecutor, stream_utils::{SharedEventStream, create_forwarding_stream}};
use crate::{
    Error,
    config::{ServiceConfig, ServiceTarget},
    health::{HealthChecker, HealthStatus},
};
use async_trait::async_trait;
use command_executor::{Command, Executor, ProcessHandle, backends::LocalLauncher, target::Target, event::ProcessEvent};
use futures::stream::{self, Stream, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use futures::lock::Mutex;
use tracing::{debug, info, warn};

/// Information about a running process
struct ProcessInfo {
    handle: Box<dyn ProcessHandle>,
    event_stream: SharedEventStream,
}

/// Executor for local process services
/// 
/// Uses interior mutability via Arc<Mutex<>> for running_processes to allow
/// modification through &self methods as required by the ServiceExecutor trait.
pub struct ProcessExecutor {
    executor: Executor<LocalLauncher>,
    health_checker: HealthChecker,
    running_processes: Arc<Mutex<HashMap<String, ProcessInfo>>>,
}

impl ProcessExecutor {
    /// Create a new process executor
    pub fn new() -> Self {
        Self {
            executor: Executor::new("process-executor".to_string(), LocalLauncher),
            health_checker: HealthChecker::new(),
            running_processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Get the number of running processes (for testing)
    #[cfg(test)]
    pub async fn running_process_count(&self) -> usize {
        self.running_processes.lock().await.len()
    }
    
    /// Check if a process is being tracked (for testing)
    #[cfg(test)]
    pub async fn is_process_tracked(&self, service_id: &str) -> bool {
        self.running_processes.lock().await.contains_key(service_id)
    }
}

impl Default for ProcessExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceExecutor for ProcessExecutor {
    async fn start(&self, config: ServiceConfig) -> std::result::Result<RunningService, Error> {
        let ServiceTarget::Process {
            binary,
            args,
            env,
            working_dir,
        } = &config.target
        else {
            return Err(crate::Error::Config(
                "ProcessExecutor can only handle Process targets".to_string(),
            ));
        };

        info!("Starting process service: {}", config.name);
        debug!("Command: {} {}", binary, args.join(" "));

        // Build command
        let mut cmd = Command::new(binary);
        cmd.args(args);

        // Set environment variables
        for (key, value) in env {
            cmd.env(key, value);
        }

        // Set working directory if specified
        if let Some(wd) = working_dir {
            cmd.current_dir(wd);
        }

        // Launch the command using ManagedProcess target to get a process handle
        let target = Target::ManagedProcess(command_executor::target::ManagedProcess::new());
        let (event_stream, handle) = self.executor.launch(&target, cmd).await?;
        
        // Get the PID from the handle
        let pid = handle.pid().unwrap_or(0);
        
        info!(
            "Started process service '{}' with PID: {}",
            config.name, pid
        );

        // Create running service instance
        let running_service = RunningService::new(config.name.clone(), config)
            .with_pid(pid)
            .with_metadata("executor_type".to_string(), "process".to_string());
            
        // Store the process handle and event stream
        {
            let mut processes = self.running_processes.lock().await;
            processes.insert(
                running_service.id.to_string(), 
                ProcessInfo {
                    handle: Box::new(handle) as Box<dyn ProcessHandle>,
                    event_stream: Arc::new(Mutex::new(Box::new(event_stream))),
                }
            );
        }

        Ok(running_service)
    }

    async fn stop(&self, service: &RunningService) -> std::result::Result<(), Error> {
        info!("Stopping service: {}", service.name);

        // Remove and get the process info
        let process_info = {
            let mut processes = self.running_processes.lock().await;
            processes.remove(&service.id.to_string())
        };

        if let Some(mut process_info) = process_info {
            // Use the handle to properly terminate the process
            match process_info.handle.terminate().await {
                Ok(_) => {
                    info!("Successfully terminated service: {}", service.name);
                }
                Err(e) => {
                    warn!("Failed to terminate service {}: {}, trying kill", service.name, e);
                    // Try force kill
                    if let Err(e) = process_info.handle.kill().await {
                        warn!("Failed to kill service {}: {}", service.name, e);
                    }
                }
            }
        } else if let Some(pid) = service.pid {
            // Fallback: use kill command if we don't have a handle
            warn!("No process handle found for service {}, using kill command", service.name);
            let mut kill_cmd = Command::new("kill");
            kill_cmd.args(&[pid.to_string()]);

            match self.executor.execute(&Target::Command, kill_cmd).await {
                Ok(result) => {
                    if result.success() {
                        info!("Successfully stopped service: {}", service.name);
                    } else {
                        warn!(
                            "Kill command failed for service: {}, trying SIGKILL",
                            service.name
                        );

                        // Try force kill
                        let mut force_kill_cmd = Command::new("kill");
                        force_kill_cmd.args(["-9", &pid.to_string()]);
                        self.executor
                            .execute(&Target::Command, force_kill_cmd)
                            .await?;
                    }
                }
                Err(e) => {
                    warn!("Failed to stop service {}: {}", service.name, e);
                    return Err(e.into());
                }
            }
        } else {
            warn!("No PID found for service: {}", service.name);
        }

        Ok(())
    }

    async fn health_check(&self, service: &RunningService) -> std::result::Result<HealthStatus, Error> {
        // Check if process is still running first
        if let Some(pid) = service.pid {
            let mut check_cmd = Command::new("kill");
            check_cmd.args(["-0", &pid.to_string()]);
            match self.executor.execute(&Target::Command, check_cmd).await {
                Ok(result) => {
                    if !result.success() {
                        return Ok(HealthStatus::Unhealthy("Process not running".to_string()));
                    }
                }
                Err(_) => {
                    return Ok(HealthStatus::Unhealthy(
                        "Failed to check process".to_string(),
                    ));
                }
            }
        }

        // If service has a health check configured, run it
        if let Some(health_check) = &service.config.health_check {
            self.health_checker.check_health(health_check).await
        } else {
            // No health check configured, assume healthy if process is running
            Ok(HealthStatus::Healthy)
        }
    }

    async fn stream_events(&self, service: &RunningService) -> std::result::Result<EventStream, Error> {
        // Get the event stream for this service
        let processes = self.running_processes.lock().await;
        let process_info = processes.get(&service.id.to_string())
            .ok_or_else(|| crate::Error::ServiceNotFound(service.name.clone()))?;
        
        let event_stream = process_info.event_stream.clone();
        drop(processes); // Release the lock early
        
        Ok(create_forwarding_stream(event_stream))
    }

    fn can_handle(&self, config: &ServiceConfig) -> bool {
        matches!(config.target, ServiceTarget::Process { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ServiceConfig, ServiceTarget};
    use std::collections::HashMap;

    #[test]
    fn test_can_handle() {
        let executor = ProcessExecutor::new();

        let process_config = ServiceConfig {
            name: "test".to_string(),
            target: ServiceTarget::Process {
                binary: "echo".to_string(),
                args: vec!["hello".to_string()],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![],
            health_check: None,
        };

        assert!(executor.can_handle(&process_config));

        let docker_config = ServiceConfig {
            name: "test".to_string(),
            target: ServiceTarget::Docker {
                image: "nginx".to_string(),
                env: HashMap::new(),
                ports: vec![],
                volumes: vec![],
            },
            dependencies: vec![],
            health_check: None,
        };

        assert!(!executor.can_handle(&docker_config));
    }

    #[smol_potat::test]
    async fn test_start_simple_process() {
        let executor = ProcessExecutor::new();

        let config = ServiceConfig {
            name: "echo-test".to_string(),
            target: ServiceTarget::Process {
                binary: "echo".to_string(),
                args: vec!["hello world".to_string()],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![],
            health_check: None,
        };

        let service = executor.start(config).await.unwrap();
        assert_eq!(service.name, "echo-test");
        assert!(service.pid.is_some());
    }

    #[smol_potat::test]
    async fn test_process_handle_storage() {
        let executor = ProcessExecutor::new();
        
        // Test that processes are stored when started
        let config1 = ServiceConfig {
            name: "test-service-1".to_string(),
            target: ServiceTarget::Process {
                binary: "sleep".to_string(),
                args: vec!["0.1".to_string()],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![],
            health_check: None,
        };
        
        let config2 = ServiceConfig {
            name: "test-service-2".to_string(), 
            target: ServiceTarget::Process {
                binary: "sleep".to_string(),
                args: vec!["0.1".to_string()],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![],
            health_check: None,
        };
        
        // Start services - this should store handles
        let service1 = executor.start(config1).await.unwrap();
        let service2 = executor.start(config2).await.unwrap();
        
        // Verify both processes are tracked
        assert_eq!(executor.running_process_count().await, 2);
        assert!(executor.is_process_tracked(&service1.id.to_string()).await);
        assert!(executor.is_process_tracked(&service2.id.to_string()).await);
        
        // Stop one service
        executor.stop(&service1).await.unwrap();
        
        // Verify it's removed from tracking
        assert_eq!(executor.running_process_count().await, 1);
        assert!(!executor.is_process_tracked(&service1.id.to_string()).await);
        assert!(executor.is_process_tracked(&service2.id.to_string()).await);
        
        // Stop the other service
        executor.stop(&service2).await.unwrap();
        assert_eq!(executor.running_process_count().await, 0);
    }

    #[smol_potat::test]
    async fn test_concurrent_process_tracking() {
        use futures::future::join_all;
        
        let executor = Arc::new(ProcessExecutor::new());
        let mut handles = vec![];
        
        // Start multiple processes concurrently
        for i in 0..5 {
            let executor_clone = executor.clone();
            let handle = smol::spawn(async move {
                let config = ServiceConfig {
                    name: format!("concurrent-test-{}", i),
                    target: ServiceTarget::Process {
                        binary: "sleep".to_string(),
                        args: vec!["0.1".to_string()],
                        env: HashMap::new(),
                        working_dir: None,
                    },
                    dependencies: vec![],
                    health_check: None,
                };
                
                executor_clone.start(config).await
            });
            handles.push(handle);
        }
        
        // Wait for all to complete
        let services: Vec<_> = join_all(handles).await.into_iter()
            .collect::<Result<Vec<_>>>().unwrap();
        
        // Verify all processes are tracked
        assert_eq!(executor.running_process_count().await, 5);
        for service in &services {
            assert!(executor.is_process_tracked(&service.id.to_string()).await);
        }
    }

    #[smol_potat::test]
    async fn test_process_cleanup_on_exit() {
        let executor = ProcessExecutor::new();
        
        // Start a process that exits quickly
        let config = ServiceConfig {
            name: "quick-exit".to_string(),
            target: ServiceTarget::Process {
                binary: "echo".to_string(),
                args: vec!["done".to_string()],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![],
            health_check: None,
        };
        
        let service = executor.start(config).await.unwrap();
        
        // Process should be tracked initially
        assert!(executor.is_process_tracked(&service.id.to_string()).await);
        
        // Wait a bit for process to exit
        smol::Timer::after(std::time::Duration::from_millis(100)).await;
        
        // Check if the process is still running (it shouldn't be)
        // This test will help us think about auto-cleanup of exited processes
        // For now, we expect manual cleanup via stop()
        assert!(executor.is_process_tracked(&service.id.to_string()).await);
        
        // Cleanup
        executor.stop(&service).await.unwrap();
        assert!(!executor.is_process_tracked(&service.id.to_string()).await);
    }

    #[smol_potat::test]
    async fn test_log_streaming_basic() {
        let executor = ProcessExecutor::new();
        
        // Start a process that produces output
        let config = ServiceConfig {
            name: "log-producer".to_string(),
            target: ServiceTarget::Process {
                binary: "sh".to_string(),
                args: vec![
                    "-c".to_string(),
                    "echo 'Starting service'; sleep 0.1; echo 'Service running'; sleep 0.1; echo 'Stopping service'".to_string()
                ],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![],
            health_check: None,
        };
        
        let service = executor.start(config).await.unwrap();
        
        // Get event stream
        let mut event_stream = executor.stream_events(&service).await.unwrap();
        
        // Collect some events
        let mut events = Vec::new();
        let timeout = std::time::Duration::from_secs(1);
        let start = std::time::Instant::now();
        
        while start.elapsed() < timeout {
            match futures::stream::StreamExt::next(&mut event_stream).await {
                Some(event) => {
                    if matches!(event.event_type, command_executor::event::ProcessEventType::Stdout | command_executor::event::ProcessEventType::Stderr) {
                        events.push(event);
                    }
                },
                None => break,
            }
        }
        
        // Verify we got some log events
        assert!(!events.is_empty(), "Expected to receive some log events");
        
        // Cleanup
        executor.stop(&service).await.unwrap();
    }

    #[smol_potat::test]
    async fn test_log_streaming_multiple_services() {
        let executor = ProcessExecutor::new();
        
        // Start multiple services
        let config1 = ServiceConfig {
            name: "service1".to_string(),
            target: ServiceTarget::Process {
                binary: "sh".to_string(),
                args: vec!["-c".to_string(), "while true; do echo 'Service 1 log'; sleep 0.2; done".to_string()],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![],
            health_check: None,
        };
        
        let config2 = ServiceConfig {
            name: "service2".to_string(),
            target: ServiceTarget::Process {
                binary: "sh".to_string(),
                args: vec!["-c".to_string(), "while true; do echo 'Service 2 log'; sleep 0.2; done".to_string()],
                env: HashMap::new(),
                working_dir: None,
            },
            dependencies: vec![],
            health_check: None,
        };
        
        let service1 = executor.start(config1).await.unwrap();
        let service2 = executor.start(config2).await.unwrap();
        
        // Get event streams for both
        let mut stream1 = executor.stream_events(&service1).await.unwrap();
        let mut stream2 = executor.stream_events(&service2).await.unwrap();
        
        // Verify we can get events from both services
        let timeout = std::time::Duration::from_millis(500);
        
        // Check service 1 events
        let event1 = smol::future::or(
            async {
                loop {
                    match futures::stream::StreamExt::next(&mut stream1).await {
                        Some(event) if matches!(event.event_type, command_executor::event::ProcessEventType::Stdout | command_executor::event::ProcessEventType::Stderr) => {
                            return Some(event);
                        },
                        Some(_) => continue,
                        None => return None,
                    }
                }
            },
            async {
                smol::Timer::after(timeout).await;
                None
            }
        ).await;
        
        // Check service 2 events
        let event2 = smol::future::or(
            async {
                loop {
                    match futures::stream::StreamExt::next(&mut stream2).await {
                        Some(event) if matches!(event.event_type, command_executor::event::ProcessEventType::Stdout | command_executor::event::ProcessEventType::Stderr) => {
                            return Some(event);
                        },
                        Some(_) => continue,
                        None => return None,
                    }
                }
            },
            async {
                smol::Timer::after(timeout).await;
                None
            }
        ).await;
        
        assert!(event1.is_some(), "Expected log events from service 1");
        assert!(event2.is_some(), "Expected log events from service 2");
        
        // Cleanup
        executor.stop(&service1).await.unwrap();
        executor.stop(&service2).await.unwrap();
    }
}
