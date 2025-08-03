//! Layered attacher implementation for composable service attachment.

use super::ExecutionContext;
use crate::{
    attacher::{AttachConfig, Attacher, AttachedHandle, ServiceStatus},
    error::Result,
    event::ProcessEvent,
    target::ManagedService,
};
use async_trait::async_trait;
use futures::stream::BoxStream;

/// Trait for attachment layers that transform service attachment behavior
#[async_trait]
pub trait AttachmentLayer: Send + Sync {
    /// Get a description of this layer for debugging
    fn description(&self) -> String;
    
    /// Transform the attach target through this layer
    fn transform_target(
        &self,
        target: ManagedService,
        context: &ExecutionContext,
    ) -> Result<ManagedService>;
    
    /// Wrap the attached handle from the inner layer
    fn wrap_handle(
        &self,
        handle: Box<dyn AttachedHandle>,
        context: &ExecutionContext,
    ) -> Result<Box<dyn AttachedHandle>>;
    
    /// Optionally transform the event stream
    fn wrap_event_stream(
        &self,
        stream: BoxStream<'static, ProcessEvent>,
        _context: &ExecutionContext,
    ) -> Result<BoxStream<'static, ProcessEvent>> {
        Ok(stream)
    }
}

/// Attacher that applies a series of attachment layers before attaching to services
pub struct LayeredAttacher<A: Attacher> {
    /// The underlying attacher that will perform the actual attachment
    attacher: A,
    /// Stack of attachment layers to apply
    layers: Vec<Box<dyn AttachmentLayer>>,
    /// Execution context
    context: ExecutionContext,
}

impl<A: Attacher> LayeredAttacher<A> 
where
    A::Target: Clone,
{
    /// Create a new layered attacher with the given attacher
    pub fn new(attacher: A) -> Self {
        Self {
            attacher,
            layers: Vec::new(),
            context: ExecutionContext::new(),
        }
    }
    
    /// Add an attachment layer to the stack
    pub fn with_layer<Layer: AttachmentLayer + 'static>(mut self, layer: Layer) -> Self {
        self.layers.push(Box::new(layer));
        self
    }
    
    /// Set the execution context
    pub fn with_context(mut self, context: ExecutionContext) -> Self {
        self.context = context;
        self
    }
    
    /// Add an environment variable to the context
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.env.insert(key.into(), value.into());
        self
    }
    
    /// Get the number of layers in the stack
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }
    
    /// Get descriptions of all layers for debugging
    pub fn layer_descriptions(&self) -> Vec<String> {
        self.layers.iter().map(|layer| layer.description()).collect()
    }
    
    /// Attach to a service by applying all layers
    pub async fn attach(
        &self,
        target: &A::Target,
        config: AttachConfig,
    ) -> Result<(BoxStream<'static, ProcessEvent>, Box<dyn AttachedHandle>)> {
        // For now, we'll do a runtime check that target is ManagedService
        // In the future, we might want to make AttachmentLayer generic over target type
        let final_target = target.clone();
        
        // Use the underlying attacher to attach
        let (event_stream, handle) = self.attacher.attach(&final_target, config).await?;
        
        // Box the event stream
        let mut boxed_stream: BoxStream<'static, ProcessEvent> = Box::pin(event_stream);
        
        // Wrap event stream through layers in forward order
        for layer in &self.layers {
            boxed_stream = layer.wrap_event_stream(boxed_stream, &self.context)?;
        }
        
        // Wrap handle through layers in reverse order
        let mut wrapped_handle: Box<dyn AttachedHandle> = Box::new(handle);
        for layer in self.layers.iter().rev() {
            wrapped_handle = layer.wrap_handle(wrapped_handle, &self.context)?;
        }
        
        Ok((boxed_stream, wrapped_handle))
    }
}

/// Example SSH attachment layer
pub struct SshAttachmentLayer {
    ssh_target: String,
}

impl SshAttachmentLayer {
    /// Create a new SSH attachment layer
    pub fn new(ssh_target: impl Into<String>) -> Self {
        Self {
            ssh_target: ssh_target.into(),
        }
    }
}

#[async_trait]
impl AttachmentLayer for SshAttachmentLayer {
    fn description(&self) -> String {
        format!("SSH({})", self.ssh_target)
    }
    
    fn transform_target(
        &self,
        mut target: ManagedService,
        _context: &ExecutionContext,
    ) -> Result<ManagedService> {
        use crate::Command;
        
        // Transform all commands to run over SSH
        let ssh_prefix = vec!["ssh".to_string(), self.ssh_target.clone()];
        
        // Transform status command
        let mut status_cmd = Command::new(&ssh_prefix[0]);
        status_cmd.args(&ssh_prefix[1..]);
        status_cmd.args(vec![
            target.status_command.get_program().to_string_lossy().to_string()
        ]);
        status_cmd.args(target.status_command.get_args());
        target.status_command = status_cmd;
        
        // Transform log command  
        let mut log_cmd = Command::new(&ssh_prefix[0]);
        log_cmd.args(&ssh_prefix[1..]);
        log_cmd.args(vec![
            target.log_command.get_program().to_string_lossy().to_string()
        ]);
        log_cmd.args(target.log_command.get_args());
        target.log_command = log_cmd;
        
        Ok(target)
    }
    
    fn wrap_handle(
        &self,
        handle: Box<dyn AttachedHandle>,
        _context: &ExecutionContext,
    ) -> Result<Box<dyn AttachedHandle>> {
        Ok(Box::new(SshWrappedHandle {
            inner: handle,
            ssh_target: self.ssh_target.clone(),
        }))
    }
}

/// Handle wrapper for SSH layer
struct SshWrappedHandle {
    inner: Box<dyn AttachedHandle>,
    ssh_target: String,
}

#[async_trait]
impl AttachedHandle for SshWrappedHandle {
    fn id(&self) -> String {
        format!("ssh:{}/{}", self.ssh_target, self.inner.id())
    }
    
    async fn status(&self) -> Result<ServiceStatus> {
        // Status is already being checked via SSH-wrapped commands
        self.inner.status().await
    }
    
    async fn disconnect(&mut self) -> Result<()> {
        self.inner.disconnect().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, backends::LocalAttacher};
    
    #[test]
    fn test_layered_attacher_creation() {
        let attacher = LayeredAttacher::new(LocalAttacher);
        assert_eq!(attacher.layer_count(), 0);
    }
    
    #[test]
    fn test_add_layers() {
        let attacher = LayeredAttacher::new(LocalAttacher)
            .with_layer(SshAttachmentLayer::new("user@host1"))
            .with_layer(SshAttachmentLayer::new("user@host2"));
        
        assert_eq!(attacher.layer_count(), 2);
        let descriptions = attacher.layer_descriptions();
        assert_eq!(descriptions.len(), 2);
        assert_eq!(descriptions[0], "SSH(user@host1)");
        assert_eq!(descriptions[1], "SSH(user@host2)");
    }
    
    #[test]
    fn test_ssh_layer_transform() {
        let layer = SshAttachmentLayer::new("user@remote");
        let context = ExecutionContext::new();
        
        let target = ManagedService::builder("test-service")
            .status_command(Command::new("systemctl").arg("is-active").arg("nginx").clone())
            .start_command(Command::new("systemctl").arg("start").arg("nginx").clone())
            .stop_command(Command::new("systemctl").arg("stop").arg("nginx").clone())
            .log_command(Command::new("journalctl").arg("-u").arg("nginx").arg("-f").clone())
            .build()
            .unwrap();
        
        let transformed = layer.transform_target(target, &context).unwrap();
        
        // Check that commands are wrapped with SSH
        assert_eq!(transformed.status_command.get_program(), "ssh");
        let status_args: Vec<_> = transformed.status_command.get_args().iter().collect();
        assert_eq!(status_args[0], "user@remote");
        assert_eq!(status_args[1], "systemctl");
        assert_eq!(status_args[2], "is-active");
        assert_eq!(status_args[3], "nginx");
        
        assert_eq!(transformed.log_command.get_program(), "ssh");
        let log_args: Vec<_> = transformed.log_command.get_args().iter().collect();
        assert_eq!(log_args[0], "user@remote");
        assert_eq!(log_args[1], "journalctl");
    }
}