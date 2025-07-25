//! WebSocket client for service registry

use crate::{
    error::{Error, Result},
    models::*,
    tls::TlsClientConfig,
};
use async_net::TcpStream;
use async_tungstenite::{client_async, WebSocketStream};
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use futures::lock::Mutex;
use tungstenite::Message;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::tls::TlsConnector;

/// WebSocket client for service registry
pub enum WsClient {
    /// Plain TCP connection
    Plain {
        ws: WebSocketStream<TcpStream>,
        addr: SocketAddr,
        pending_requests: Arc<Mutex<HashMap<String, futures::channel::oneshot::Sender<Result<serde_json::Value>>>>>,
    },
    /// TLS connection
    Tls {
        ws: WebSocketStream<async_tls::client::TlsStream<TcpStream>>,
        addr: SocketAddr,
        pending_requests: Arc<Mutex<HashMap<String, futures::channel::oneshot::Sender<Result<serde_json::Value>>>>>,
    },
}

impl WsClient {
    /// Connect to a WebSocket server (plain HTTP)
    pub async fn connect(addr: SocketAddr) -> Result<Self> {
        let url = format!("ws://{}", addr);
        let stream = TcpStream::connect(addr).await?;
        let (ws, _) = client_async(&url, stream).await?;
        
        info!("Connected to WebSocket server at {} (no TLS)", addr);
        
        Ok(Self::Plain {
            ws,
            addr,
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    
    /// Connect to a WebSocket server with TLS
    pub async fn connect_tls(addr: SocketAddr, tls_config: TlsClientConfig, server_name: &str) -> Result<Self> {
        let url = format!("wss://{}", addr);
        let tcp_stream = TcpStream::connect(addr).await?;
        
        // Establish TLS connection
        let connector = TlsConnector::from(tls_config.config);
        let tls_stream = connector.connect(server_name, tcp_stream).await?;
        
        // WebSocket handshake over TLS
        let (ws, _) = client_async(&url, tls_stream).await?;
        
        info!("Connected to WebSocket server at {} (with TLS)", addr);
        
        Ok(Self::Tls {
            ws,
            addr,
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    
    /// Start the message handler (run in background)
    pub async fn start_handler(self) -> (WsClientHandle, futures::future::BoxFuture<'static, Result<()>>) {
        match self {
            Self::Plain { mut ws, addr, pending_requests } => {
                let pending_requests_clone = pending_requests.clone();
                let (tx, rx) = futures::channel::mpsc::unbounded();
                
                let handle = WsClientHandle {
                    tx,
                    pending_requests: pending_requests_clone.clone(),
                };
                
                let handler = async move {
                    let mut rx = rx;
                    
                    loop {
                        futures::select! {
                            // Handle outgoing messages
                            msg = rx.next() => {
                                match msg {
                                    Some(ClientMessage::Request(msg)) => {
                                        let json = serde_json::to_string(&msg)?;
                                        ws.send(Message::Text(json)).await?;
                                    }
                                    Some(ClientMessage::Close) => {
                                        ws.send(Message::Close(None)).await?;
                                        break;
                                    }
                                    None => break,
                                }
                            }
                            
                            // Handle incoming messages
                            msg = ws.next() => {
                                match msg {
                                    Some(Ok(Message::Text(text))) => {
                                        if let Err(e) = Self::handle_message(&text, &pending_requests).await {
                                            error!("Error handling message: {}", e);
                                        }
                                    }
                                    Some(Ok(Message::Close(_))) => {
                                        info!("Server closed connection");
                                        break;
                                    }
                                    Some(Err(e)) => {
                                        error!("WebSocket error: {}", e);
                                        break;
                                    }
                                    None => break,
                                    _ => {}
                                }
                            }
                        }
                    }
                    
                    Ok(())
                };
                
                (handle, Box::pin(handler))
            }
            Self::Tls { mut ws, addr, pending_requests } => {
                let pending_requests_clone = pending_requests.clone();
                let (tx, rx) = futures::channel::mpsc::unbounded();
                
                let handle = WsClientHandle {
                    tx,
                    pending_requests: pending_requests_clone.clone(),
                };
                
                let handler = async move {
                    let mut rx = rx;
                    
                    loop {
                        futures::select! {
                            // Handle outgoing messages
                            msg = rx.next() => {
                                match msg {
                                    Some(ClientMessage::Request(msg)) => {
                                        let json = serde_json::to_string(&msg)?;
                                        ws.send(Message::Text(json)).await?;
                                    }
                                    Some(ClientMessage::Close) => {
                                        ws.send(Message::Close(None)).await?;
                                        break;
                                    }
                                    None => break,
                                }
                            }
                            
                            // Handle incoming messages
                            msg = ws.next() => {
                                match msg {
                                    Some(Ok(Message::Text(text))) => {
                                        if let Err(e) = Self::handle_message(&text, &pending_requests).await {
                                            error!("Error handling message: {}", e);
                                        }
                                    }
                                    Some(Ok(Message::Close(_))) => {
                                        info!("Server closed connection");
                                        break;
                                    }
                                    Some(Err(e)) => {
                                        error!("WebSocket error: {}", e);
                                        break;
                                    }
                                    None => break,
                                    _ => {}
                                }
                            }
                        }
                    }
                    
                    Ok(())
                };
                
                (handle, Box::pin(handler))
            }
        }
    }
    
    /// Handle incoming message (static to be used by both variants)
    async fn handle_message(
        text: &str,
        pending: &Arc<Mutex<HashMap<String, futures::channel::oneshot::Sender<Result<serde_json::Value>>>>>
    ) -> Result<()> {
        let msg: WsMessage = serde_json::from_str(text)?;
        
        match msg {
            WsMessage::Response { id, data, error } => {
                let mut pending = pending.lock().await;
                if let Some(tx) = pending.remove(&id) {
                    if let Some(error) = error {
                        let _ = tx.send(Err(Error::Package(error.message)));
                    } else if let Some(data) = data {
                        let _ = tx.send(Ok(data));
                    } else {
                        let _ = tx.send(Err(Error::Package("Empty response".to_string())));
                    }
                }
            }
            WsMessage::Event { event, data } => {
                // For now, just log events
                debug!("Received event {:?}: {:?}", event, data);
            }
            _ => {
                debug!("Unexpected message type");
            }
        }
        
        Ok(())
    }
}

/// Handle for interacting with the WebSocket client
#[derive(Clone)]
pub struct WsClientHandle {
    tx: futures::channel::mpsc::UnboundedSender<ClientMessage>,
    pending_requests: Arc<Mutex<HashMap<String, futures::channel::oneshot::Sender<Result<serde_json::Value>>>>>,
}

enum ClientMessage {
    Request(WsMessage),
    Close,
}

impl WsClientHandle {
    /// Send a request and wait for response
    async fn request(&self, action: Action, params: serde_json::Value) -> Result<serde_json::Value> {
        let id = Uuid::new_v4().to_string();
        let (tx, rx) = futures::channel::oneshot::channel();
        
        // Register pending request
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(id.clone(), tx);
        }
        
        // Send request
        let msg = WsMessage::Request {
            id: id.clone(),
            action,
            params,
        };
        
        self.tx.unbounded_send(ClientMessage::Request(msg))
            .map_err(|_| Error::Package("Failed to send request".to_string()))?;
        
        // Wait for response
        match rx.await {
            Ok(result) => result,
            Err(_) => {
                // Clean up if cancelled
                let mut pending = self.pending_requests.lock().await;
                pending.remove(&id);
                Err(Error::Package("Request cancelled".to_string()))
            }
        }
    }
    
    /// List all services
    pub async fn list_services(&self) -> Result<Vec<ServiceEntry>> {
        let data = self.request(Action::ListServices, serde_json::json!({})).await?;
        Ok(serde_json::from_value(data)?)
    }
    
    /// Get a specific service
    pub async fn get_service(&self, name: &str) -> Result<ServiceEntry> {
        let params = serde_json::json!({ "name": name });
        let data = self.request(Action::GetService, params).await?;
        Ok(serde_json::from_value(data)?)
    }
    
    /// Perform action on a service
    pub async fn service_action(&self, name: &str, action: ServiceAction) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "name": name,
            "action": action,
        });
        self.request(Action::ServiceAction, params).await
    }
    
    /// List all endpoints
    pub async fn list_endpoints(&self) -> Result<HashMap<String, Vec<Endpoint>>> {
        let data = self.request(Action::ListEndpoints, serde_json::json!({})).await?;
        Ok(serde_json::from_value(data)?)
    }
    
    /// Subscribe to events
    pub async fn subscribe(&self, events: Vec<EventType>) -> Result<Vec<EventType>> {
        let params = serde_json::json!({ "events": events });
        let data = self.request(Action::Subscribe, params).await?;
        
        if let Some(subscribed) = data.get("subscribed") {
            Ok(serde_json::from_value(subscribed.clone())?)
        } else {
            Err(Error::Package("Invalid subscribe response".to_string()))
        }
    }
    
    /// Unsubscribe from events
    pub async fn unsubscribe(&self, events: Vec<EventType>) -> Result<Vec<EventType>> {
        let params = serde_json::json!({ "events": events });
        let data = self.request(Action::Unsubscribe, params).await?;
        
        if let Some(subscribed) = data.get("subscribed") {
            Ok(serde_json::from_value(subscribed.clone())?)
        } else {
            Err(Error::Package("Invalid unsubscribe response".to_string()))
        }
    }
    
    /// Deploy a package
    pub async fn deploy_package(&self, package_path: &str, target_node: Option<&str>) -> Result<serde_json::Value> {
        let mut params = serde_json::json!({ "package_path": package_path });
        if let Some(node) = target_node {
            params["target_node"] = serde_json::json!(node);
        }
        self.request(Action::DeployPackage, params).await
    }
    
    /// Close the connection
    pub async fn close(&self) -> Result<()> {
        self.tx.unbounded_send(ClientMessage::Close)
            .map_err(|_| Error::Package("Failed to send close".to_string()))?;
        Ok(())
    }
}