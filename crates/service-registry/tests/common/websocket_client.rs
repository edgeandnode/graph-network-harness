//! WebSocket test client wrapper

use service_registry::{WsClient, WsClientHandle, EventType, ServiceAction};
use std::net::SocketAddr;
use serde_json::Value;
use anyhow::Result;

/// WebSocket test client wrapper
pub struct WebSocketTestClient {
    handle: WsClientHandle,
    _handler_task: smol::Task<Result<()>>,
}

impl WebSocketTestClient {
    /// Connect to WebSocket server
    pub async fn connect(addr: SocketAddr) -> Result<Self> {
        let client = WsClient::connect(addr).await?;
        let (handle, handler) = client.start_handler().await;
        
        // Run handler in background
        let handler_task = smol::spawn(async move {
            handler.await?;
            Ok(())
        });
        
        Ok(Self {
            handle,
            _handler_task: handler_task,
        })
    }
    
    /// Subscribe to events
    pub async fn subscribe(&self, events: Vec<EventType>) -> Result<()> {
        self.handle.subscribe(events).await?;
        Ok(())
    }
    
    /// Unsubscribe from events
    pub async fn unsubscribe(&self, events: Vec<EventType>) -> Result<()> {
        self.handle.unsubscribe(events).await?;
        Ok(())
    }
    
    /// List services
    pub async fn list_services(&self) -> Result<Value> {
        let services = self.handle.list_services().await?;
        Ok(serde_json::to_value(services)?)
    }
    
    /// Get a service
    pub async fn get_service(&self, name: &str) -> Result<Value> {
        let service = self.handle.get_service(name).await?;
        Ok(serde_json::to_value(service)?)
    }
    
    /// List endpoints
    pub async fn list_endpoints(&self) -> Result<Value> {
        let endpoints = self.handle.list_endpoints().await?;
        Ok(serde_json::to_value(endpoints)?)
    }
    
    /// Deploy a package
    pub async fn deploy_package(&self, package_path: &str, target_node: Option<&str>) -> Result<Value> {
        self.handle.deploy_package(package_path, target_node).await
            .map_err(|e| anyhow::anyhow!("Deploy failed: {}", e))
    }
    
    /// Start a service
    pub async fn start_service(&self, name: &str) -> Result<()> {
        self.handle.service_action(name, ServiceAction::Start).await?;
        Ok(())
    }
    
    /// Stop a service
    pub async fn stop_service(&self, name: &str) -> Result<()> {
        self.handle.service_action(name, ServiceAction::Stop).await?;
        Ok(())
    }
    
    /// Close the connection
    pub async fn close(self) -> Result<()> {
        self.handle.close().await?;
        Ok(())
    }
}