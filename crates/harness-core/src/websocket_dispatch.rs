//! WebSocket JSON dispatch layer
//!
//! This module handles WebSocket messages for service action dispatch.
//! It routes incoming JSON action requests to the appropriate services
//! and streams back responses.

use async_net::{TcpListener, TcpStream};
use async_trait::async_trait;
use async_tungstenite::{WebSocketStream, accept_async, tungstenite::Message};
use futures::{FutureExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, error, warn};

use crate::Error;
use crate::service::JsonServiceRegistry;
use crate::tls::{TlsAcceptor, TlsServerConfig};
use std::result::Result;

/// JSON-RPC style request for service actions
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActionRequest {
    /// Unique request ID for correlation
    pub id: String,
    /// Service instance name
    pub service: String,
    /// Action name to execute
    pub action: String,
    /// Action parameters as JSON
    pub params: Value,
}

/// JSON-RPC style response for service actions
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActionResponse {
    /// Request ID this response correlates to
    pub id: String,
    /// Result of the action (if successful)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Error message (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Event emitted during action execution
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActionEvent {
    /// Request ID this event relates to
    pub id: String,
    /// Service that emitted the event
    pub service: String,
    /// Event data
    pub event: Value,
}

/// WebSocket message types
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum WebSocketMessage {
    /// Request to execute an action
    Request(ActionRequest),
    /// Response from an action
    Response(ActionResponse),
    /// Event emitted during action execution
    Event(ActionEvent),
    /// List available services
    ListServices,
    /// Response to ListServices
    Services {
        /// List of available services
        services: Vec<ServiceInfo>,
    },
    /// List available actions for a service
    ListActions {
        /// Name of the service to list actions for
        service: String,
    },
    /// Response to ListActions
    Actions {
        /// Name of the service
        service: String,
        /// List of available actions for the service
        actions: Vec<ActionInfo>,
    },
    /// Validate if setup is complete for a service
    ValidateSetup {
        /// Name of the service to validate
        service: String,
    },
    /// Perform setup for a service
    PerformSetup {
        /// Name of the service to set up
        service: String,
    },
    /// Response to setup operations
    SetupStatus {
        /// Name of the service
        service: String,
        /// Whether the setup is valid
        valid: bool,
        /// Error message if setup failed
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

/// Information about a service
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceInfo {
    /// Service instance name
    pub name: String,
    /// Service description
    pub description: String,
    /// Number of available actions
    pub action_count: usize,
    /// Whether the service implements ServiceSetup
    pub has_setup: bool,
    /// Whether the service implements ServiceEvents
    pub has_events: bool,
    /// Schema of events if the service emits them
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_schema: Option<Value>,
}

/// Information about an action
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActionInfo {
    /// Action name
    pub name: String,
    /// Action description
    pub description: String,
    /// Input schema
    pub input_schema: Value,
    /// Response schema
    pub response_schema: Value,
}

/// WebSocket message dispatcher
pub struct WebSocketDispatcher {
    registry: Arc<JsonServiceRegistry>,
}

impl WebSocketDispatcher {
    /// Create a new dispatcher with a service registry
    pub fn new(registry: Arc<JsonServiceRegistry>) -> Self {
        Self { registry }
    }

    /// Handle an incoming WebSocket message
    pub async fn handle_message(
        &self,
        message: WebSocketMessage,
    ) -> Result<Vec<WebSocketMessage>, Error> {
        match message {
            WebSocketMessage::Request(request) => self.handle_action_request(request).await,
            WebSocketMessage::ListServices => Ok(vec![self.handle_list_services()]),
            WebSocketMessage::ListActions { service } => {
                Ok(vec![self.handle_list_actions(&service)?])
            }
            WebSocketMessage::ValidateSetup { service } => {
                Ok(vec![self.handle_validate_setup(&service).await?])
            }
            WebSocketMessage::PerformSetup { service } => {
                Ok(vec![self.handle_perform_setup(&service).await?])
            }
            _ => {
                // Other message types are outbound only
                Err(Error::service_type("Invalid inbound message type"))
            }
        }
    }

    /// Handle an action request
    async fn handle_action_request(
        &self,
        request: ActionRequest,
    ) -> Result<Vec<WebSocketMessage>, Error> {
        let mut messages = Vec::new();

        // Dispatch the action
        match self
            .registry
            .dispatch(&request.service, &request.action, request.params.clone())
            .await
        {
            Ok((result_rx, _converter)) => {
                // Wait for the result
                match result_rx.recv().await {
                    Ok(result) => {
                        messages.push(WebSocketMessage::Response(ActionResponse {
                            id: request.id,
                            result: Some(result),
                            error: None,
                        }));
                    }
                    Err(e) => {
                        messages.push(WebSocketMessage::Response(ActionResponse {
                            id: request.id,
                            result: None,
                            error: Some(format!("Failed to receive result: {}", e)),
                        }));
                    }
                }
            }
            Err(e) => {
                messages.push(WebSocketMessage::Response(ActionResponse {
                    id: request.id,
                    result: None,
                    error: Some(e.to_string()),
                }));
            }
        }

        Ok(messages)
    }

    /// Handle list services request
    fn handle_list_services(&self) -> WebSocketMessage {
        let services: Vec<ServiceInfo> = self
            .registry
            .list()
            .into_iter()
            .map(|(name, service)| ServiceInfo {
                name: name.to_string(),
                description: service.description().to_string(),
                action_count: service.available_actions().len(),
                has_setup: service.has_setup(),
                has_events: service.has_events(),
                event_schema: service.event_schema(),
            })
            .collect();

        WebSocketMessage::Services { services }
    }

    /// Handle list actions request
    fn handle_list_actions(&self, service_name: &str) -> Result<WebSocketMessage, Error> {
        let service = self
            .registry
            .get(service_name)
            .ok_or_else(|| Error::service_type(format!("Service '{}' not found", service_name)))?;

        let actions: Vec<ActionInfo> = service
            .available_actions()
            .into_iter()
            .map(|action| ActionInfo {
                name: action.name,
                description: action.description,
                input_schema: action.input_schema,
                response_schema: action.event_schema, // Using event_schema as response for now
            })
            .collect();

        Ok(WebSocketMessage::Actions {
            service: service_name.to_string(),
            actions,
        })
    }

    /// Handle validate setup request
    async fn handle_validate_setup(&self, service_name: &str) -> Result<WebSocketMessage, Error> {
        let service = self
            .registry
            .get(service_name)
            .ok_or_else(|| Error::service_type(format!("Service '{}' not found", service_name)))?;

        if !service.has_setup() {
            return Ok(WebSocketMessage::SetupStatus {
                service: service_name.to_string(),
                valid: false,
                error: Some("Service does not implement ServiceSetup".to_string()),
            });
        }

        match service.validate_setup().await {
            Ok(()) => Ok(WebSocketMessage::SetupStatus {
                service: service_name.to_string(),
                valid: true,
                error: None,
            }),
            Err(e) => Ok(WebSocketMessage::SetupStatus {
                service: service_name.to_string(),
                valid: false,
                error: Some(e.to_string()),
            }),
        }
    }

    /// Handle perform setup request
    async fn handle_perform_setup(&self, service_name: &str) -> Result<WebSocketMessage, Error> {
        let service = self
            .registry
            .get(service_name)
            .ok_or_else(|| Error::service_type(format!("Service '{}' not found", service_name)))?;

        if !service.has_setup() {
            return Ok(WebSocketMessage::SetupStatus {
                service: service_name.to_string(),
                valid: false,
                error: Some("Service does not implement ServiceSetup".to_string()),
            });
        }

        // First validate to check if setup is needed (idempotency)
        if service.validate_setup().await.is_ok() {
            // Setup already complete
            return Ok(WebSocketMessage::SetupStatus {
                service: service_name.to_string(),
                valid: true,
                error: None,
            });
        }

        // Perform the setup
        match service.perform_setup().await {
            Ok(()) => {
                // Validate again to confirm success
                match service.validate_setup().await {
                    Ok(()) => Ok(WebSocketMessage::SetupStatus {
                        service: service_name.to_string(),
                        valid: true,
                        error: None,
                    }),
                    Err(e) => Ok(WebSocketMessage::SetupStatus {
                        service: service_name.to_string(),
                        valid: false,
                        error: Some(format!("Setup completed but validation failed: {}", e)),
                    }),
                }
            }
            Err(e) => Ok(WebSocketMessage::SetupStatus {
                service: service_name.to_string(),
                valid: false,
                error: Some(format!("Setup failed: {}", e)),
            }),
        }
    }
}

/// Trait for WebSocket transport implementations
#[async_trait]
pub trait WebSocketTransport: Send + Sync {
    /// Send a message over the WebSocket
    async fn send(&self, message: WebSocketMessage) -> Result<(), Error>;

    /// Receive a message from the WebSocket
    async fn recv(&self) -> Result<WebSocketMessage, Error>;

    /// Check if the connection is still open
    fn is_connected(&self) -> bool;
}

/// WebSocket server that handles incoming connections
pub struct WebSocketServer {
    dispatcher: Arc<WebSocketDispatcher>,
    address: std::net::SocketAddr,
    tls_config: Option<TlsServerConfig>,
    shutdown_rx: async_channel::Receiver<()>,
    shutdown_tx: async_channel::Sender<()>,
}

impl WebSocketServer {
    /// Create a new WebSocket server (plain HTTP)
    pub fn new(registry: Arc<JsonServiceRegistry>, address: std::net::SocketAddr) -> Self {
        let (shutdown_tx, shutdown_rx) = async_channel::bounded(1);
        Self {
            dispatcher: Arc::new(WebSocketDispatcher::new(registry)),
            address,
            tls_config: None,
            shutdown_rx,
            shutdown_tx,
        }
    }

    /// Create a new WebSocket server with TLS
    pub fn new_tls(
        registry: Arc<JsonServiceRegistry>,
        address: std::net::SocketAddr,
        tls_config: TlsServerConfig,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = async_channel::bounded(1);
        Self {
            dispatcher: Arc::new(WebSocketDispatcher::new(registry)),
            address,
            tls_config: Some(tls_config),
            shutdown_rx,
            shutdown_tx,
        }
    }

    /// Get a shutdown handle for this server
    pub fn shutdown_handle(&self) -> async_channel::Sender<()> {
        self.shutdown_tx.clone()
    }

    /// Run the WebSocket server
    pub async fn run(self) -> Result<(), Error> {
        let listener = TcpListener::bind(self.address)
            .await
            .map_err(|e| Error::service_type(format!("Failed to bind WebSocket server: {}", e)))?;

        debug!("WebSocket server listening on {}", self.address);

        loop {
            futures::select! {
                result = listener.accept().fuse() => {
                    match result {
                        Ok((stream, addr)) => {
                            debug!("New WebSocket connection from {}", addr);
                            let dispatcher = self.dispatcher.clone();
                            let tls_config = self.tls_config.clone();

                            // Spawn a task to handle this connection
                            smol::spawn(async move {
                                if let Err(e) = handle_connection(stream, dispatcher, tls_config).await {
                                    error!("Error handling WebSocket connection from {}: {}", addr, e);
                                }
                            }).detach();
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                        }
                    }
                }
                _ = self.shutdown_rx.recv().fuse() => {
                    debug!("WebSocket server shutting down");
                    break;
                }
            }
        }

        Ok(())
    }
}

/// Handle a single WebSocket connection
async fn handle_connection(
    stream: async_net::TcpStream,
    dispatcher: Arc<WebSocketDispatcher>,
    tls_config: Option<TlsServerConfig>,
) -> Result<(), Error> {
    // Handle TLS if configured
    use futures::SinkExt;

    enum WsStream {
        Plain(WebSocketStream<TcpStream>),
        Tls(WebSocketStream<futures_rustls::server::TlsStream<TcpStream>>),
    }

    let ws_stream = match tls_config {
        Some(config) => {
            let acceptor = TlsAcceptor::from(config.config);
            let tls_stream = acceptor
                .accept(stream)
                .await
                .map_err(|e| Error::service_type(format!("TLS handshake failed: {}", e)))?;
            let ws = accept_async(tls_stream)
                .await
                .map_err(|e| Error::service_type(format!("WebSocket handshake failed: {}", e)))?;
            WsStream::Tls(ws)
        }
        None => {
            let ws = accept_async(stream)
                .await
                .map_err(|e| Error::service_type(format!("WebSocket handshake failed: {}", e)))?;
            WsStream::Plain(ws)
        }
    };

    let (mut ws_sender, mut ws_receiver) = match ws_stream {
        WsStream::Plain(ws) => {
            let (s, r) = ws.split();
            (
                Box::new(s) as Box<dyn futures::Sink<Message, Error = _> + Send + Unpin>,
                Box::new(r) as Box<dyn futures::Stream<Item = Result<Message, _>> + Send + Unpin>,
            )
        }
        WsStream::Tls(ws) => {
            let (s, r) = ws.split();
            (
                Box::new(s) as Box<dyn futures::Sink<Message, Error = _> + Send + Unpin>,
                Box::new(r) as Box<dyn futures::Stream<Item = Result<Message, _>> + Send + Unpin>,
            )
        }
    };
    let (tx, rx) = async_channel::unbounded();

    // Spawn a task to send messages
    let send_task = smol::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if let Err(e) = ws_sender.send(msg).await {
                error!("Failed to send WebSocket message: {}", e);
                break;
            }
        }
    });

    // Process incoming messages
    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                // Parse the JSON message
                match serde_json::from_str::<WebSocketMessage>(&text) {
                    Ok(ws_msg) => {
                        // Handle the message
                        match dispatcher.handle_message(ws_msg).await {
                            Ok(responses) => {
                                // Send all response messages
                                for response in responses {
                                    let json = serde_json::to_string(&response)
                                        .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));
                                    if tx.send(Message::Text(json.into())).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Error handling message: {}", e);
                                let error_response = serde_json::json!({
                                    "type": "Error",
                                    "error": e.to_string()
                                });
                                let _ = tx
                                    .send(Message::Text(error_response.to_string().into()))
                                    .await;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse WebSocket message: {}", e);
                        let error_response = serde_json::json!({
                            "type": "Error",
                            "error": format!("Invalid message format: {}", e)
                        });
                        let _ = tx
                            .send(Message::Text(error_response.to_string().into()))
                            .await;
                    }
                }
            }
            Ok(Message::Close(_)) => {
                debug!("WebSocket connection closed");
                break;
            }
            Ok(_) => {
                // Ignore other message types (Binary, Ping, Pong)
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
        }
    }

    // Close the sender channel and wait for send task to finish
    drop(tx);
    send_task.await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_serialization() {
        let request = ActionRequest {
            id: "req-1".to_string(),
            service: "anvil".to_string(),
            action: "mine_blocks".to_string(),
            params: serde_json::json!({ "count": 10 }),
        };

        let message = WebSocketMessage::Request(request);
        let json = serde_json::to_string(&message).unwrap();

        assert!(json.contains("\"type\":\"Request\""));
        assert!(json.contains("\"id\":\"req-1\""));
        assert!(json.contains("\"service\":\"anvil\""));
        assert!(json.contains("\"action\":\"mine_blocks\""));
    }

    #[test]
    fn test_response_serialization() {
        let response = ActionResponse {
            id: "req-1".to_string(),
            result: Some(serde_json::json!({"blocks": 10})),
            error: None,
        };

        let message = WebSocketMessage::Response(response);
        let json = serde_json::to_string(&message).unwrap();

        assert!(json.contains("\"type\":\"Response\""));
        assert!(json.contains("\"result\""));
        assert!(!json.contains("\"error\""));
    }
}
