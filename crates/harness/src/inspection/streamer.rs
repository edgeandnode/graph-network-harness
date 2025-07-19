//! Async streaming API for real-time service event inspection

use bollard::container::LogsOptions;
use bollard::Docker;
use futures_util::{Stream, StreamExt};
use pin_project_lite::pin_project;
use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use super::events::ServiceEvent;
use super::registry::ServiceEventRegistry;

/// Main service inspector that provides async streaming of service events
pub struct ServiceInspector {
    /// Event handler registry
    registry: ServiceEventRegistry,
    /// Docker client for container operations
    docker: Docker,
    /// Active service streamers
    active_streamers: HashMap<String, ServiceStreamer>,
    /// Event broadcast channel
    event_sender: Option<mpsc::UnboundedSender<ServiceEvent>>,
}

impl ServiceInspector {
    /// Create a new service inspector
    pub fn new(docker: Docker) -> Self {
        Self {
            registry: ServiceEventRegistry::new(),
            docker,
            active_streamers: HashMap::new(),
            event_sender: None,
        }
    }

    /// Create a new service inspector with custom registry
    pub fn with_registry(docker: Docker, registry: ServiceEventRegistry) -> Self {
        Self {
            registry,
            docker,
            active_streamers: HashMap::new(),
            event_sender: None,
        }
    }

    /// Register a service event handler
    pub fn register_handler(&mut self, handler: Box<dyn crate::inspection::ServiceEventHandler>) {
        self.registry.register_handler(handler);
    }

    /// Start streaming events from all services
    pub async fn start_streaming(&mut self, container_ids: Vec<(String, String)>) -> anyhow::Result<()> {
        let (sender, receiver) = mpsc::unbounded_channel::<RawServiceEvent>();
        let (event_sender, _event_receiver) = mpsc::unbounded_channel::<ServiceEvent>();
        self.event_sender = Some(event_sender);

        for (service_name, container_id) in container_ids {
            let streamer = ServiceStreamer::new(
                self.docker.clone(),
                service_name.clone(),
                container_id,
                sender.clone(),
            );
            
            info!("Starting event streaming for service: {}", service_name);
            self.active_streamers.insert(service_name, streamer);
        }

        // Start processing events
        self.start_event_processing(receiver).await?;
        
        Ok(())
    }

    /// Get a stream of all service events
    pub fn event_stream(&self) -> impl Stream<Item = ServiceEvent> + '_ {
        let (_sender, receiver) = mpsc::unbounded_channel();
        
        // This is a simplified version - in practice you'd want to tap into the existing event flow
        tokio_stream::wrappers::UnboundedReceiverStream::new(receiver)
    }

    /// Get a stream of events for a specific service
    pub fn service_stream<'a>(&'a self, service_name: &'a str) -> impl Stream<Item = ServiceEvent> + 'a {
        let service_name = service_name.to_string();
        self.event_stream()
            .filter(move |event| {
                let matches = event.service_name == service_name;
                async move { matches }
            })
    }

    /// Start processing events from the receiver
    async fn start_event_processing(&mut self, mut receiver: mpsc::UnboundedReceiver<RawServiceEvent>) -> anyhow::Result<()> {
        // Clone the registry to move into the async task
        let registry = ServiceEventRegistry::new();
        // Copy handlers from self.registry to the new registry
        for _service_name in self.registry.registered_services() {
            // This is a simplified approach - in practice you'd want a proper clone method
        }
        
        tokio::spawn(async move {
            while let Some(raw_event) = receiver.recv().await {
                match raw_event {
                    RawServiceEvent::LogLine { service_name, line } => {
                        if let Some(event) = registry.process_log_line(&service_name, &line).await {
                            debug!("Generated event from log: {:?}", event);
                            // In a full implementation, you'd broadcast this event
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Stop streaming for a specific service
    pub async fn stop_service_streaming(&mut self, service_name: &str) -> anyhow::Result<()> {
        if let Some(streamer) = self.active_streamers.remove(service_name) {
            info!("Stopping event streaming for service: {}", service_name);
            streamer.stop().await?;
        }
        Ok(())
    }

    /// Stop all streaming
    pub async fn stop_all_streaming(&mut self) -> anyhow::Result<()> {
        let service_names: Vec<String> = self.active_streamers.keys().cloned().collect();
        
        for service_name in service_names {
            self.stop_service_streaming(&service_name).await?;
        }
        
        info!("Stopped all service event streaming");
        Ok(())
    }

    /// Get streaming statistics
    pub fn streaming_stats(&self) -> StreamingStats {
        StreamingStats {
            active_services: self.active_streamers.len(),
            registered_handlers: self.registry.stats().total_handlers,
            service_names: self.active_streamers.keys().cloned().collect(),
        }
    }
}

/// Individual service streamer that handles log streaming for one service
pub struct ServiceStreamer {
    docker: Docker,
    service_name: String,
    container_id: String,
    event_sender: mpsc::UnboundedSender<RawServiceEvent>,
    stop_handle: Option<tokio::task::JoinHandle<()>>,
}

impl ServiceStreamer {
    /// Create a new service streamer
    pub fn new(
        docker: Docker,
        service_name: String,
        container_id: String,
        event_sender: mpsc::UnboundedSender<RawServiceEvent>,
    ) -> Self {
        let mut streamer = Self {
            docker,
            service_name,
            container_id,
            event_sender,
            stop_handle: None,
        };
        
        streamer.start_streaming();
        streamer
    }

    /// Start streaming logs from the container
    fn start_streaming(&mut self) {
        let docker = self.docker.clone();
        let container_id = self.container_id.clone();
        let service_name = self.service_name.clone();
        let sender = self.event_sender.clone();

        let handle = tokio::spawn(async move {
            let options = LogsOptions::<String> {
                follow: true,
                stdout: true,
                stderr: true,
                timestamps: true,
                ..Default::default()
            };

            let mut stream = docker.logs(&container_id, Some(options));
            while let Some(log_result) = stream.next().await {
                match log_result {
                    Ok(log_output) => {
                        let log_line = log_output.to_string();
                        let raw_event = RawServiceEvent::LogLine {
                            service_name: service_name.clone(),
                            line: log_line,
                        };
                        
                        if sender.send(raw_event).is_err() {
                            debug!("Event receiver dropped, stopping log streaming for {}", service_name);
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Error reading logs for {}: {}", service_name, e);
                        break;
                    }
                }
            }
            
            debug!("Log streaming ended for service: {}", service_name);
        });

        self.stop_handle = Some(handle);
    }

    /// Stop streaming
    pub async fn stop(self) -> anyhow::Result<()> {
        if let Some(handle) = self.stop_handle {
            handle.abort();
            match handle.await {
                Ok(_) => debug!("Service streamer stopped cleanly for {}", self.service_name),
                Err(e) if e.is_cancelled() => debug!("Service streamer cancelled for {}", self.service_name),
                Err(e) => warn!("Service streamer error for {}: {}", self.service_name, e),
            }
        }
        Ok(())
    }
}

/// Raw service events before processing through handlers
#[derive(Debug, Clone)]
pub enum RawServiceEvent {
    LogLine {
        service_name: String,
        line: String,
    },
}

/// Statistics about active streaming
#[derive(Debug, Clone)]
pub struct StreamingStats {
    /// Number of services currently being streamed
    pub active_services: usize,
    /// Number of registered event handlers
    pub registered_handlers: usize,
    /// Names of services being streamed
    pub service_names: Vec<String>,
}

// Custom stream implementation for filtered events
pin_project! {
    pub struct FilteredEventStream<S> {
        #[pin]
        stream: S,
        filter_fn: Box<dyn Fn(&ServiceEvent) -> bool + Send + Sync>,
    }
}

impl<S> FilteredEventStream<S> {
    pub fn new(stream: S, filter_fn: Box<dyn Fn(&ServiceEvent) -> bool + Send + Sync>) -> Self {
        Self { stream, filter_fn }
    }
}

impl<S> Stream for FilteredEventStream<S>
where
    S: Stream<Item = ServiceEvent>,
{
    type Item = ServiceEvent;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        
        loop {
            match this.stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(event)) => {
                    if (this.filter_fn)(&event) {
                        return Poll::Ready(Some(event));
                    }
                    // Continue polling if event doesn't match filter
                }
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspection::handlers::{GenericEventHandler, PostgresEventHandler};

    #[tokio::test]
    async fn test_inspector_creation() {
        let docker = Docker::connect_with_local_defaults().unwrap();
        let inspector = ServiceInspector::new(docker);
        
        let stats = inspector.streaming_stats();
        assert_eq!(stats.active_services, 0);
        assert_eq!(stats.registered_handlers, 0);
    }

    #[tokio::test]
    async fn test_handler_registration() {
        let docker = Docker::connect_with_local_defaults().unwrap();
        let mut inspector = ServiceInspector::new(docker);
        
        inspector.register_handler(Box::new(PostgresEventHandler::new()));
        
        let stats = inspector.streaming_stats();
        assert_eq!(stats.registered_handlers, 1);
    }
}