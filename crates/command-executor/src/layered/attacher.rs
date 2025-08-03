//! Layered attacher implementation for composable service attachment.

use super::ExecutionContext;
use crate::{
    attacher::{AttachConfig, AttachedHandle, Attacher, ServiceStatus},
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
        self.layers
            .iter()
            .map(|layer| layer.description())
            .collect()
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

/// SSH attachment layer for attaching to services over SSH
pub struct SshAttachmentLayer {
    ssh_target: String,
}

/// Docker attachment layer for attaching to existing containers
pub struct DockerAttachmentLayer {
    container_id: String,
}

/// Local attachment layer for local services
pub struct LocalAttachmentLayer {
    service_prefix: Option<String>,
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
            target
                .status_command
                .get_program()
                .to_string_lossy()
                .to_string(),
        ]);
        status_cmd.args(target.status_command.get_args());
        target.status_command = status_cmd;

        // Transform log command
        let mut log_cmd = Command::new(&ssh_prefix[0]);
        log_cmd.args(&ssh_prefix[1..]);
        log_cmd.args(vec![
            target
                .log_command
                .get_program()
                .to_string_lossy()
                .to_string(),
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

impl DockerAttachmentLayer {
    /// Create a new Docker attachment layer
    pub fn new(container_id: impl Into<String>) -> Self {
        Self {
            container_id: container_id.into(),
        }
    }
}

#[async_trait]
impl AttachmentLayer for DockerAttachmentLayer {
    fn description(&self) -> String {
        format!("Docker({})", self.container_id)
    }

    fn transform_target(
        &self,
        mut target: ManagedService,
        _context: &ExecutionContext,
    ) -> Result<ManagedService> {
        use crate::Command;

        // Transform commands to use docker exec
        let _docker_prefix = vec![
            "docker".to_string(),
            "exec".to_string(),
            self.container_id.clone(),
        ];

        // Transform status command - check if container is running
        target.status_command = Command::new("docker")
            .arg("inspect")
            .arg("-f")
            .arg("{{.State.Running}}")
            .arg(&self.container_id)
            .clone();

        // Transform log command
        target.log_command = Command::new("docker")
            .arg("logs")
            .arg("-f")
            .arg(&self.container_id)
            .clone();

        Ok(target)
    }

    fn wrap_handle(
        &self,
        handle: Box<dyn AttachedHandle>,
        _context: &ExecutionContext,
    ) -> Result<Box<dyn AttachedHandle>> {
        Ok(Box::new(DockerWrappedHandle {
            inner: handle,
            container_id: self.container_id.clone(),
        }))
    }
}

/// Handle wrapper for Docker layer
struct DockerWrappedHandle {
    inner: Box<dyn AttachedHandle>,
    container_id: String,
}

#[async_trait]
impl AttachedHandle for DockerWrappedHandle {
    fn id(&self) -> String {
        format!("docker:{}/{}", self.container_id, self.inner.id())
    }

    async fn status(&self) -> Result<ServiceStatus> {
        self.inner.status().await
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.inner.disconnect().await
    }
}

impl LocalAttachmentLayer {
    /// Create a new local attachment layer
    pub fn new() -> Self {
        Self {
            service_prefix: None,
        }
    }

    /// Create with a service prefix
    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            service_prefix: Some(prefix.into()),
        }
    }
}

#[async_trait]
impl AttachmentLayer for LocalAttachmentLayer {
    fn description(&self) -> String {
        match &self.service_prefix {
            Some(prefix) => format!("Local({})", prefix),
            None => "Local".to_string(),
        }
    }

    fn transform_target(
        &self,
        target: ManagedService,
        _context: &ExecutionContext,
    ) -> Result<ManagedService> {
        // For local attachment, we typically don't need to transform the target
        // The prefix is more for identification in wrapped handles

        Ok(target)
    }

    fn wrap_handle(
        &self,
        handle: Box<dyn AttachedHandle>,
        _context: &ExecutionContext,
    ) -> Result<Box<dyn AttachedHandle>> {
        // For local, we don't need to wrap much
        Ok(handle)
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
            .status_command(
                Command::new("systemctl")
                    .arg("is-active")
                    .arg("nginx")
                    .clone(),
            )
            .start_command(Command::new("systemctl").arg("start").arg("nginx").clone())
            .stop_command(Command::new("systemctl").arg("stop").arg("nginx").clone())
            .log_command(
                Command::new("journalctl")
                    .arg("-u")
                    .arg("nginx")
                    .arg("-f")
                    .clone(),
            )
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

    #[test]
    fn test_docker_layer_transform() {
        let layer = DockerAttachmentLayer::new("my-container");
        let context = ExecutionContext::new();

        let target = ManagedService::builder("test-service")
            .status_command(Command::new("ps").arg("aux").clone())
            .start_command(Command::new("unused").clone())
            .stop_command(Command::new("unused").clone())
            .log_command(
                Command::new("tail")
                    .arg("-f")
                    .arg("/var/log/app.log")
                    .clone(),
            )
            .build()
            .unwrap();

        let transformed = layer.transform_target(target, &context).unwrap();

        // Check that status command uses docker inspect
        assert_eq!(transformed.status_command.get_program(), "docker");
        let status_args: Vec<_> = transformed.status_command.get_args().iter().collect();
        assert_eq!(status_args[0], "inspect");
        assert_eq!(status_args[1], "-f");
        assert_eq!(status_args[2], "{{.State.Running}}");
        assert_eq!(status_args[3], "my-container");

        // Check that log command uses docker logs
        assert_eq!(transformed.log_command.get_program(), "docker");
        let log_args: Vec<_> = transformed.log_command.get_args().iter().collect();
        assert_eq!(log_args[0], "logs");
        assert_eq!(log_args[1], "-f");
        assert_eq!(log_args[2], "my-container");
    }

    #[test]
    fn test_local_layer_transform() {
        let layer = LocalAttachmentLayer::with_prefix("test-");
        let context = ExecutionContext::new();

        let target = ManagedService::builder("service")
            .status_command(
                Command::new("systemctl")
                    .arg("is-active")
                    .arg("nginx")
                    .clone(),
            )
            .start_command(Command::new("systemctl").arg("start").arg("nginx").clone())
            .stop_command(Command::new("systemctl").arg("stop").arg("nginx").clone())
            .log_command(Command::new("journalctl").arg("-u").arg("nginx").clone())
            .build()
            .unwrap();

        let transformed = layer.transform_target(target.clone(), &context).unwrap();

        // Local layer doesn't transform commands, just passes through
        assert_eq!(
            transformed.status_command.get_program(),
            target.status_command.get_program()
        );
        assert_eq!(
            transformed.log_command.get_program(),
            target.log_command.get_program()
        );
    }

    #[test]
    fn test_layer_descriptions() {
        let ssh_layer = SshAttachmentLayer::new("user@host");
        assert_eq!(ssh_layer.description(), "SSH(user@host)");

        let docker_layer = DockerAttachmentLayer::new("container-123");
        assert_eq!(docker_layer.description(), "Docker(container-123)");

        let local_layer = LocalAttachmentLayer::new();
        assert_eq!(local_layer.description(), "Local");

        let local_with_prefix = LocalAttachmentLayer::with_prefix("prod-");
        assert_eq!(local_with_prefix.description(), "Local(prod-)");
    }
}
