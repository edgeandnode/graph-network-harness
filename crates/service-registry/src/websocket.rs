//! WebSocket server implementation

use crate::{
    error::{Error, Result},
    models::*,
    registry::Registry,
    tls::TlsServerConfig,
};
use async_net::{TcpListener, TcpStream};
use async_tungstenite::{accept_async, WebSocketStream};
use futures::StreamExt;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use tungstenite::Message;

use crate::tls::TlsAcceptor;

/// WebSocket server
pub struct WsServer {
    registry: Arc<Registry>,
    /// The TCP listener
    pub listener: TcpListener,
    /// TLS configuration (if enabled)
    tls_config: Option<TlsServerConfig>,
}

impl WsServer {
    /// Create a new WebSocket server (plain HTTP)
    pub async fn new(addr: impl AsRef<str>, registry: Registry) -> Result<Self> {
        let listener = TcpListener::bind(addr.as_ref()).await?;
        info!("WebSocket server listening on {} (no TLS)", addr.as_ref());

        Ok(Self {
            registry: Arc::new(registry),
            listener,
            tls_config: None,
        })
    }

    /// Create a new WebSocket server with TLS
    pub async fn new_tls(
        addr: impl AsRef<str>,
        registry: Registry,
        tls_config: TlsServerConfig,
    ) -> Result<Self> {
        let listener = TcpListener::bind(addr.as_ref()).await?;
        info!("WebSocket server listening on {} (with TLS)", addr.as_ref());

        Ok(Self {
            registry: Arc::new(registry),
            listener,
            tls_config: Some(tls_config),
        })
    }

    /// Accept a new connection
    pub async fn accept(&self) -> Result<ConnectionHandler> {
        let (tcp_stream, addr) = self.listener.accept().await?;

        // Create appropriate connection handler based on TLS config
        match &self.tls_config {
            Some(tls_config) => {
                let acceptor = TlsAcceptor::from(tls_config.config.clone());
                let tls_stream = acceptor.accept(tcp_stream).await?;
                let ws_stream = accept_async(tls_stream).await?;

                debug!("New WebSocket connection from {} (TLS)", addr);

                Ok(ConnectionHandler::Tls(TlsConnectionHandler {
                    ws: ws_stream,
                    addr,
                    registry: self.registry.clone(),
                    subscriptions: HashSet::new(),
                }))
            }
            None => {
                let ws_stream = accept_async(tcp_stream).await?;

                debug!("New WebSocket connection from {} (plain)", addr);

                Ok(ConnectionHandler::Plain(PlainConnectionHandler {
                    ws: ws_stream,
                    addr,
                    registry: self.registry.clone(),
                    subscriptions: HashSet::new(),
                }))
            }
        }
    }

    /// Get the registry reference
    pub fn registry(&self) -> &Arc<Registry> {
        &self.registry
    }
}

/// WebSocket connection handler
pub enum ConnectionHandler {
    /// Plain TCP connection
    Plain(PlainConnectionHandler),
    /// TLS connection
    Tls(TlsConnectionHandler),
}

impl ConnectionHandler {
    /// Handle the connection
    pub async fn handle(self) -> Result<()> {
        match self {
            ConnectionHandler::Plain(handler) => handler.handle().await,
            ConnectionHandler::Tls(handler) => handler.handle().await,
        }
    }
}

/// Plain TCP connection handler
pub struct PlainConnectionHandler {
    ws: WebSocketStream<TcpStream>,
    addr: SocketAddr,
    registry: Arc<Registry>,
    subscriptions: HashSet<EventType>,
}

/// TLS connection handler
pub struct TlsConnectionHandler {
    ws: WebSocketStream<futures_rustls::server::TlsStream<TcpStream>>,
    addr: SocketAddr,
    registry: Arc<Registry>,
    subscriptions: HashSet<EventType>,
}

// Macro to implement common handler logic
macro_rules! impl_connection_handler {
    ($handler_type:ty) => {
        impl $handler_type {
            /// Handle the connection
            pub async fn handle(mut self) -> Result<()> {
                info!("Handling connection from {}", self.addr);

                // Send initial state
                self.send_initial_state().await?;

                // Process messages
                while let Some(msg) = self.ws.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            if let Err(e) = self.process_text_message(&text).await {
                                error!("Error processing message: {}", e);
                                self.send_error_response("", &e).await?;
                            }
                        }
                        Ok(Message::Close(_)) => {
                            debug!("Client {} closing connection", self.addr);
                            break;
                        }
                        Ok(Message::Ping(data)) => {
                            self.ws.send(Message::Pong(data)).await?;
                        }
                        Ok(_) => {
                            // Ignore other message types
                        }
                        Err(e) => {
                            error!("WebSocket error from {}: {}", self.addr, e);
                            break;
                        }
                    }
                }

                // Clean up subscriptions
                self.registry.remove_subscriber(self.addr).await?;

                info!("Connection from {} closed", self.addr);
                Ok(())
            }

            /// Send initial registry state
            async fn send_initial_state(&mut self) -> Result<()> {
                let services = self.registry.list().await;

                let event = WsMessage::Event {
                    event: EventType::RegistryLoaded,
                    data: serde_json::to_value(&services)?,
                };

                self.send_message(&event).await
            }

            /// Process a text message
            async fn process_text_message(&mut self, text: &str) -> Result<()> {
                let msg: WsMessage = serde_json::from_str(text)?;

                match msg {
                    WsMessage::Request { id, action, params } => {
                        self.handle_request(&id, action, params).await?;
                    }
                    _ => {
                        warn!("Unexpected message type from client");
                    }
                }

                Ok(())
            }

            /// Handle a request
            async fn handle_request(&mut self, id: &str, action: Action, params: serde_json::Value) -> Result<()> {
                debug!("Request {}: {:?}", id, action);

                let response = match action {
                    Action::ListServices => self.handle_list_services().await,
                    Action::GetService => self.handle_get_service(params).await,
                    Action::ServiceAction => self.handle_service_action(params).await,
                    Action::ListEndpoints => self.handle_list_endpoints().await,
                    Action::Subscribe => self.handle_subscribe(params).await,
                    Action::Unsubscribe => self.handle_unsubscribe(params).await,
                    Action::DeployPackage => self.handle_deploy_package(params).await,
                };

                match response {
                    Ok(data) => self.send_response(id, data).await?,
                    Err(e) => self.send_error_response(id, &e).await?,
                }

                Ok(())
            }

            /// Handle list services request
            async fn handle_list_services(&self) -> Result<serde_json::Value> {
                let services = self.registry.list().await;
                Ok(serde_json::to_value(&services)?)
            }

            /// Handle get service request
            async fn handle_get_service(&self, params: serde_json::Value) -> Result<serde_json::Value> {
                #[derive(Deserialize)]
                struct GetServiceParams {
                    name: String,
                }

                let params: GetServiceParams = serde_json::from_value(params)?;
                let service = self.registry.get(&params.name).await?;
                Ok(serde_json::to_value(&service)?)
            }

            /// Handle service action request
            async fn handle_service_action(&self, params: serde_json::Value) -> Result<serde_json::Value> {
                #[derive(Deserialize)]
                struct ServiceActionParams {
                    name: String,
                    action: ServiceAction,
                }

                let params: ServiceActionParams = serde_json::from_value(params)?;

                // Map action to state transition
                let new_state = match params.action {
                    ServiceAction::Start => ServiceState::Starting,
                    ServiceAction::Stop => ServiceState::Stopping,
                    ServiceAction::Restart => ServiceState::Starting,
                    ServiceAction::Reload => {
                        // Reload doesn't change state
                        return Ok(serde_json::json!({ "status": "reload_initiated" }));
                    }
                };

                let (old_state, _events) = self.registry.update_state(&params.name, new_state).await?;

                Ok(serde_json::json!({
                    "service": params.name,
                    "old_state": old_state,
                    "new_state": new_state,
                }))
            }

            /// Handle list endpoints request
            async fn handle_list_endpoints(&self) -> Result<serde_json::Value> {
                let endpoints = self.registry.list_endpoints().await;
                Ok(serde_json::to_value(&endpoints)?)
            }

            /// Handle subscribe request
            async fn handle_subscribe(&mut self, params: serde_json::Value) -> Result<serde_json::Value> {
                #[derive(Deserialize)]
                struct SubscribeParams {
                    events: Vec<EventType>,
                }

                let params: SubscribeParams = serde_json::from_value(params)?;

                // Update local subscriptions
                for event in &params.events {
                    self.subscriptions.insert(*event);
                }

                // Subscribe to registry events
                self.registry.subscribe(self.addr, params.events).await?;

                Ok(serde_json::json!({
                    "subscribed": self.subscriptions.iter().collect::<Vec<_>>(),
                }))
            }

            /// Handle unsubscribe request
            async fn handle_unsubscribe(&mut self, params: serde_json::Value) -> Result<serde_json::Value> {
                #[derive(Deserialize)]
                struct UnsubscribeParams {
                    events: Vec<EventType>,
                }

                let params: UnsubscribeParams = serde_json::from_value(params)?;

                // Update local subscriptions
                for event in &params.events {
                    self.subscriptions.remove(event);
                }

                // Unsubscribe from registry events
                self.registry.unsubscribe(self.addr, params.events).await?;

                Ok(serde_json::json!({
                    "subscribed": self.subscriptions.iter().collect::<Vec<_>>(),
                }))
            }

            /// Handle deploy package request (stub for now)
            async fn handle_deploy_package(&self, _params: serde_json::Value) -> Result<serde_json::Value> {
                // TODO: Implement package deployment
                Err(Error::Package("Package deployment not yet implemented".to_string()))
            }

            /// Send a response
            async fn send_response(&mut self, id: &str, data: serde_json::Value) -> Result<()> {
                let msg = WsMessage::Response {
                    id: id.to_string(),
                    data: Some(data),
                    error: None,
                };

                self.send_message(&msg).await
            }

            /// Send an error response
            async fn send_error_response(&mut self, id: &str, error: &Error) -> Result<()> {
                let msg = WsMessage::Response {
                    id: id.to_string(),
                    data: None,
                    error: Some(ErrorInfo {
                        code: "error".to_string(),
                        message: error.to_string(),
                        details: None,
                    }),
                };

                self.send_message(&msg).await
            }

            /// Send a message
            async fn send_message(&mut self, msg: &WsMessage) -> Result<()> {
                let json = serde_json::to_string(msg)?;
                self.ws.send(Message::Text(json.into())).await?;
                Ok(())
            }

            /// Check if subscribed to an event
            pub fn is_subscribed(&self, event_type: &EventType) -> bool {
                self.subscriptions.contains(event_type)
            }

            /// Send an event if subscribed
            pub async fn send_event(&mut self, event: EventType, data: serde_json::Value) -> Result<()> {
                if self.is_subscribed(&event) {
                    let msg = WsMessage::Event { event, data };
                    self.send_message(&msg).await?;
                }
                Ok(())
            }
        }
    };
}

// Implement the handler logic for both types
impl_connection_handler!(PlainConnectionHandler);
impl_connection_handler!(TlsConnectionHandler);

use serde::Deserialize;
