//! Layered task executor for flexible execution with composed layers

use crate::{OrchestrationError, TaskConfig, TaskExecutor, ServiceTarget};
use crate::executors::layered::LayerConfig;
use async_channel::Receiver;
use async_runtime_compat::Spawner;
use async_trait::async_trait;
use command_executor::{
    Command, ProcessHandle,
    layered::{DockerLayer, LocalLayer, SshLayer},
};
use futures::StreamExt;
use serde_json::Value as JsonValue;
use std::result::Result;
use tracing::{debug, info, error};

/// Task executor for layered execution
pub struct LayeredTaskExecutor {}

impl LayeredTaskExecutor {
    /// Create a new layered task executor
    pub fn new() -> Self {
        Self {}
    }
    
    /// Build a layered executor from configuration
    fn build_executor(&self, layers: &[LayerConfig]) -> Result<command_executor::layered::LayeredExecutor<command_executor::backends::LocalLauncher>, OrchestrationError> {
        let launcher = command_executor::backends::LocalLauncher;
        let mut executor = command_executor::layered::LayeredExecutor::new(launcher);
        
        for layer in layers {
            match layer {
                LayerConfig::Local { env, working_dir } => {
                    let mut local_layer = LocalLayer::new();
                    for (key, value) in env {
                        local_layer = local_layer.with_env(key.clone(), value.clone());
                    }
                    if let Some(dir) = working_dir {
                        local_layer = local_layer.with_working_dir(dir.clone());
                    }
                    executor = executor.with_layer(local_layer);
                }
                LayerConfig::Ssh { host, user, env, port, identity_file, options } => {
                    let destination = format!("{}@{}", user, host);
                    let mut ssh_layer = SshLayer::new(destination);
                    if let Some(p) = port {
                        ssh_layer = ssh_layer.with_port(*p);
                    }
                    if let Some(key) = identity_file {
                        ssh_layer = ssh_layer.with_identity_file(key.clone());
                    }
                    for opt in options {
                        ssh_layer = ssh_layer.with_option(opt.clone());
                    }
                    for (key, value) in env {
                        ssh_layer = ssh_layer.with_env(key.clone(), value.clone());
                    }
                    executor = executor.with_layer(ssh_layer);
                }
                LayerConfig::Docker { container, user, working_dir, env, interactive, tty } => {
                    let mut docker_layer = DockerLayer::new(container.clone());
                    if let Some(u) = user {
                        docker_layer = docker_layer.with_user(u.clone());
                    }
                    if let Some(dir) = working_dir {
                        docker_layer = docker_layer.with_working_dir(dir.clone());
                    }
                    for (key, value) in env {
                        docker_layer = docker_layer.with_env(key.clone(), value.clone());
                    }
                    if *interactive {
                        docker_layer = docker_layer.with_interactive(true);
                    }
                    if *tty {
                        docker_layer = docker_layer.with_tty(true);
                    }
                    executor = executor.with_layer(docker_layer);
                }
            }
        }
        
        Ok(executor)
    }
    
    /// Run validation command through layers
    async fn run_validation(&self, config: &TaskConfig) -> Result<bool, OrchestrationError> {
        if let ServiceTarget::Layered { layers, params, validation, .. } = &config.target {
            if let Some(validation_cmd) = validation {
                debug!("Running layered validation command: {}", validation_cmd);
                
                let executor = self.build_executor(layers)?;
                
                // Create the validation command
                let mut cmd = Command::new("sh");
                cmd.arg("-c").arg(validation_cmd);
                
                // Add any params as environment variables
                for (key, value) in params {
                    let val_str = value.as_string();
                    cmd.env(key, val_str);
                }
                
                // Execute through layers
                match executor.execute_command(cmd).await {
                    Ok((_stream, mut handle)) => {
                        let exit = handle.wait().await.map_err(|e| {
                            OrchestrationError::Config(format!("Failed to wait for validation: {}", e))
                        })?;
                        Ok(exit.success())
                    }
                    Err(e) => {
                        debug!("Validation command failed: {}", e);
                        Ok(false)
                    }
                }
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }
}

#[async_trait]
impl TaskExecutor for LayeredTaskExecutor {
    fn can_handle(&self, config: &TaskConfig) -> bool {
        matches!(config.target, ServiceTarget::Layered { .. })
    }
    
    async fn is_complete(&self, _name: &str, config: &TaskConfig) -> Result<bool, OrchestrationError> {
        self.run_validation(config).await
    }
    
    async fn execute(
        &self,
        name: &str,
        config: &TaskConfig,
        spawner: &dyn Spawner,
    ) -> Result<Receiver<JsonValue>, OrchestrationError> {
        info!("Executing layered task: {}", name);
        
        // Extract layered configuration
        let (layers, params, command_template) = match &config.target {
            ServiceTarget::Layered { layers, params, command_template, .. } => {
                (layers, params, command_template.clone())
            }
            _ => {
                return Err(OrchestrationError::Config(
                    format!("LayeredTaskExecutor cannot handle non-layered target")
                ));
            }
        };
        
        // Build the layered executor
        let executor = self.build_executor(layers)?;
        
        // Expand command template with params
        let mut command = command_template.clone();
        if let Some(cmd_str) = command_template {
            for (key, value) in params {
                let val_str = value.as_string();
                command = Some(cmd_str.replace(&format!("{{{}}}", key), &val_str));
            }
        }
        
        // Create channel for state updates
        let (tx, rx) = async_channel::unbounded();
        
        // Clone what we need for the spawned task
        let task_name = name.to_string();
        let tx_clone = tx.clone();
        let params_clone = params.clone();
        
        // Spawn the task execution
        spawner.spawn_detached(Box::pin(async move {
            // Send initial state
            let _ = tx_clone.send(serde_json::json!("Running")).await;
            
            // Create the command
            let mut cmd = Command::new("sh");
            if let Some(ref cmd_str) = command {
                cmd.arg("-c").arg(cmd_str);
            }
            
            // Add params as environment variables
            for (key, value) in &params_clone {
                let val_str = value.as_string();
                cmd.env(key, val_str);
            }
            
            // Execute through layers
            match executor.execute_command(cmd).await {
                Ok((mut event_stream, _handle)) => {
                    // Stream events from the process
                    while let Some(event) = event_stream.next().await {
                        use command_executor::ProcessEventType;
                        match event.event_type {
                            ProcessEventType::Stdout => {
                                if let Some(ref data) = event.data {
                                    debug!("Task {} stdout: {}", task_name, data);
                                }
                            }
                            ProcessEventType::Stderr => {
                                if let Some(ref data) = event.data {
                                    debug!("Task {} stderr: {}", task_name, data);
                                }
                            }
                            ProcessEventType::Exited { code, .. } => {
                                if code == Some(0) {
                                    info!("Task {} completed successfully", task_name);
                                    let _ = tx_clone.send(serde_json::json!("Completed")).await;
                                } else {
                                    error!("Task {} failed with exit code: {:?}", task_name, code);
                                    let _ = tx_clone.send(serde_json::json!("Failed")).await;
                                }
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to execute task {}: {}", task_name, e);
                    let _ = tx_clone.send(serde_json::json!("Failed")).await;
                }
            }
        }));
        
        Ok(rx)
    }
}