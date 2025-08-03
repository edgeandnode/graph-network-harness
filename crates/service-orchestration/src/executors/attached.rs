//! Attached service implementations using command-executor's Attacher trait.
//!
//! This module provides executors that attach to existing services rather than
//! spawning new ones.

use super::traits::{AttachedService, EventStreamable, EventStream};
use super::RunningService;
use crate::{Error, config::{ServiceConfig, ServiceTarget}};
use async_trait::async_trait;
use command_executor::{
    attacher::{Attacher, AttachConfig, AttachedHandle, ServiceStatus as AttacherStatus},
    backends::{LocalAttacher, LocalLauncher},
    target::Target,
    Command,
};
use std::collections::HashMap;
use std::sync::Arc;
use futures::lock::Mutex;
use tracing::info;

/// Executor that attaches to existing systemd services
pub struct SystemdAttachedExecutor {
    attacher: LocalAttacher,
    attached_services: Arc<Mutex<HashMap<String, Box<dyn AttachedHandle>>>>,
}

impl SystemdAttachedExecutor {
    /// Create a new systemd attached executor
    pub fn new() -> Self {
        Self {
            attacher: LocalAttacher,
            attached_services: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl EventStreamable for SystemdAttachedExecutor {
    async fn stream_events(&self, service: &RunningService) -> std::result::Result<EventStream, Error> {
        let attached = self.attached_services.lock().await;
        if let Some(_handle) = attached.get(&service.name) {
            // Get service name from metadata
            if let Some(unit_name) = service.metadata.get("systemd_unit") {
                // Create command to stream journald logs
                let cmd = Command::new("journalctl")
                    .arg("-u")
                    .arg(unit_name)
                    .arg("-f")  // Follow mode
                    .arg("-n")
                    .arg("0")   // Start from end
                    .clone();
                
                // Use local executor to run journalctl
                let executor = command_executor::Executor::new(
                    format!("journald-{}", unit_name), 
                    LocalLauncher
                );
                
                let (events, _handle) = executor.launch(&Target::Command, cmd).await?;
                
                // Store handle somewhere if we need to stop it later
                // For now, let it run until dropped
                
                Ok(Box::pin(events))
            } else {
                Err(Error::NotImplemented("Event streaming not implemented for this service".to_string()))
            }
        } else {
            Err(Error::NotImplemented("Event streaming not implemented for this service".to_string()))
        }
    }
}

#[async_trait]
impl AttachedService for SystemdAttachedExecutor {
    async fn attach(&self, config: ServiceConfig) -> std::result::Result<RunningService, Error> {
        // Extract service name from config
        let env = config.target.env();
        let service_name = env.get("SYSTEMD_SERVICE")
            .ok_or_else(|| crate::Error::Config("SYSTEMD_SERVICE not specified".into()))?;
            
        info!("Attaching to systemd service: {}", service_name);
        
        // Create attached service target for systemd
        let target = command_executor::target::AttachedService::builder(service_name)
            .status_command(Command::new("systemctl").arg("is-active").arg(service_name).clone())
            .log_command(Command::new("journalctl").arg("-u").arg(service_name).arg("-f").clone())
            .build()?;
        
        // Attach to the service
        let attach_config = AttachConfig::default();
        let (event_stream, handle) = self.attacher.attach(&target, attach_config).await?;
        
        // Store the handle
        let service_id = handle.id();
        self.attached_services.lock().await.insert(config.name.clone(), Box::new(handle));
        
        // Create running service info
        let service = RunningService::new(config.name.clone(), config)
            .with_metadata("systemd_unit".to_string(), service_name.clone())
            .with_metadata("attached_id".to_string(), service_id);
        
        Ok(service)
    }
    
    async fn detach(&self, service: &RunningService) -> std::result::Result<(), Error> {
        info!("Detaching from systemd service: {}", service.name);
        
        let mut attached = self.attached_services.lock().await;
        if let Some(mut handle) = attached.remove(&service.name) {
            handle.disconnect().await?;
        }
        
        Ok(())
    }
    
    async fn is_accessible(&self, service: &RunningService) -> std::result::Result<bool, Error> {
        let attached = self.attached_services.lock().await;
        if let Some(handle) = attached.get(&service.name) {
            let status = handle.status().await?;
            Ok(matches!(status, AttacherStatus::Running))
        } else {
            Ok(false)
        }
    }
    
    fn can_handle(&self, config: &ServiceConfig) -> bool {
        config.target.env().contains_key("SYSTEMD_SERVICE")
    }
}

// Note: Health checking can be implemented separately using the HealthCheckable trait
// from the health module if needed

/// Executor that attaches to existing Docker containers
pub struct DockerAttachedExecutor {
    // In a real implementation, would use docker attacher when available
    attached_containers: Arc<Mutex<HashMap<String, String>>>,
}

impl DockerAttachedExecutor {
    /// Create a new Docker attached executor
    pub fn new() -> Self {
        Self {
            attached_containers: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl EventStreamable for DockerAttachedExecutor {
    async fn stream_events(&self, service: &RunningService) -> std::result::Result<EventStream, Error> {
        if let Some(container_id) = &service.container_id {
            // Create command to stream docker logs
            let cmd = Command::new("docker")
                .arg("logs")
                .arg("-f")  // Follow mode
                .arg("--tail")
                .arg("0")   // Start from end
                .arg(container_id)
                .clone();
            
            // Use local executor to run docker logs
            let executor = command_executor::Executor::new(
                format!("docker-logs-{}", container_id), 
                LocalLauncher
            );
            
            let (events, _handle) = executor.launch(&Target::Command, cmd).await?;
            
            // Store handle somewhere if we need to stop it later
            // For now, let it run until dropped
            
            Ok(Box::pin(events))
        } else {
            Err(Error::NotImplemented("Event streaming not implemented for this service".to_string()))
        }
    }
}

#[async_trait]
impl AttachedService for DockerAttachedExecutor {
    async fn attach(&self, config: ServiceConfig) -> std::result::Result<RunningService, Error> {
        match &config.target {
            ServiceTarget::Docker { .. } => {
                let env = config.target.env();
                let container_name = env.get("CONTAINER_NAME")
                    .ok_or_else(|| crate::Error::Config("CONTAINER_NAME not specified for attachment".into()))?;
                    
                info!("Attaching to Docker container: {}", container_name);
                
                // In real implementation, would find container by name
                // and verify it exists and is running
                self.attached_containers.lock().await
                    .insert(config.name.clone(), container_name.clone());
                
                let mut service = RunningService::new(config.name.clone(), config);
                service.container_id = Some(container_name.clone());
                
                Ok(service)
            }
            _ => Err(crate::Error::Config("Docker attacher requires Docker target".into()))
        }
    }
    
    async fn detach(&self, service: &RunningService) -> std::result::Result<(), Error> {
        info!("Detaching from Docker container: {}", service.name);
        self.attached_containers.lock().await.remove(&service.name);
        Ok(())
    }
    
    async fn is_accessible(&self, service: &RunningService) -> std::result::Result<bool, Error> {
        // Would check if container is still running via Docker API
        let attached = self.attached_containers.lock().await;
        Ok(attached.contains_key(&service.name))
    }
    
    fn can_handle(&self, config: &ServiceConfig) -> bool {
        matches!(config.target, ServiceTarget::Docker { .. }) 
            && config.target.env().contains_key("CONTAINER_NAME")
    }
}

/// Executor that attaches to existing local processes by PID or name
pub struct LocalProcessAttachedExecutor {
    executor: command_executor::Executor<LocalLauncher>,
    attached_processes: Arc<Mutex<HashMap<String, u32>>>, // service_name -> pid
}

impl LocalProcessAttachedExecutor {
    /// Create a new local process attached executor
    pub fn new() -> Self {
        Self {
            executor: command_executor::Executor::new("local-process-attacher".to_string(), LocalLauncher),
            attached_processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Find PID by process name using pgrep
    async fn find_pid_by_name(&self, process_name: &str) -> std::result::Result<u32, Error> {
        let cmd = Command::new("pgrep")
            .arg("-f")
            .arg(process_name)
            .clone();
            
        let result = self.executor.execute(&command_executor::target::Target::Command, cmd).await?;
        
        if result.success() {
            let output = result.output;
            let first_line = output.lines().next()
                .ok_or_else(|| Error::Config("No matching process found".to_string()))?;
            
            first_line.trim().parse::<u32>()
                .map_err(|_| Error::Config("Invalid PID format".to_string()))
        } else {
            Err(Error::Config(format!("Process '{}' not found", process_name)))
        }
    }
    
    /// Check if a process is running by PID
    async fn is_process_running(&self, pid: u32) -> std::result::Result<bool, Error> {
        let cmd = Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .clone();
            
        let result = self.executor.execute(&command_executor::target::Target::Command, cmd).await?;
        Ok(result.success())
    }
}

#[async_trait]
impl EventStreamable for LocalProcessAttachedExecutor {
    async fn stream_events(&self, service: &RunningService) -> std::result::Result<EventStream, Error> {
        if let Some(pid) = service.pid {
            // Use journalctl to follow logs for the process by PID
            let cmd = Command::new("journalctl")
                .arg(format!("_PID={}", pid))
                .arg("-f")  // Follow mode
                .arg("-n")
                .arg("0")   // Start from end
                .clone();
                
            let (events, _handle) = self.executor.launch(&command_executor::target::Target::Command, cmd).await?;
            
            // Note: We're dropping the handle here, which means we can't stop the journalctl process
            // This is a limitation of the current design
            
            Ok(Box::pin(events))
        } else {
            Err(Error::NotImplemented("Event streaming requires PID".to_string()))
        }
    }
}

#[async_trait]
impl AttachedService for LocalProcessAttachedExecutor {
    async fn attach(&self, config: ServiceConfig) -> std::result::Result<RunningService, Error> {
        let env = config.target.env();
        
        // Try to get PID or process name from config
        let pid = if let Some(pid_str) = env.get("PID") {
            pid_str.parse::<u32>()
                .map_err(|_| Error::Config("Invalid PID format".to_string()))?
        } else if let Some(process_name) = env.get("PROCESS_NAME") {
            self.find_pid_by_name(process_name).await?
        } else {
            return Err(Error::Config("Either PID or PROCESS_NAME must be specified".to_string()));
        };
        
        // Verify the process is running
        if !self.is_process_running(pid).await? {
            return Err(Error::Config(format!("Process with PID {} is not running", pid)));
        }
        
        info!("Attaching to local process with PID: {}", pid);
        
        // Store the attached process
        self.attached_processes.lock().await
            .insert(config.name.clone(), pid);
        
        let service = RunningService::new(config.name.clone(), config)
            .with_pid(pid)
            .with_metadata("attachment_type".to_string(), "local_process".to_string());
        
        Ok(service)
    }
    
    async fn detach(&self, service: &RunningService) -> std::result::Result<(), Error> {
        info!("Detaching from local process: {}", service.name);
        self.attached_processes.lock().await.remove(&service.name);
        Ok(())
    }
    
    async fn is_accessible(&self, service: &RunningService) -> std::result::Result<bool, Error> {
        if let Some(pid) = service.pid {
            self.is_process_running(pid).await
        } else {
            Ok(false)
        }
    }
    
    fn can_handle(&self, config: &ServiceConfig) -> bool {
        let env = config.target.env();
        env.contains_key("PID") || env.contains_key("PROCESS_NAME")
    }
}