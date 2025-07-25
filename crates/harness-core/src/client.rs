//! Client libraries for communicating with harness daemons
//!
//! This module provides client abstractions for connecting to and interacting
//! with harness daemons, including action invocation and event streaming.

use async_trait::async_trait;
use serde_json::Value;
use std::net::SocketAddr;
use tracing::{debug, info};

use crate::{Error, Result};

/// Trait for clients that can communicate with harness daemons
#[async_trait]
pub trait Client: Send + Sync {
    /// Connect to a daemon
    async fn connect(endpoint: SocketAddr) -> Result<Self>
    where
        Self: Sized;
    
    /// Invoke an action on the daemon
    async fn action(&self, name: &str, params: Value) -> Result<Value>;
    
    /// List available actions
    async fn list_actions(&self) -> Result<Vec<Value>>;
    
    /// Disconnect from the daemon
    async fn disconnect(&self) -> Result<()>;
}

/// Test client implementation for integration testing
pub struct TestClient {
    endpoint: SocketAddr,
    connected: bool,
}

impl TestClient {
    /// Create a new test client (not connected)
    pub fn new(endpoint: SocketAddr) -> Self {
        Self {
            endpoint,
            connected: false,
        }
    }
    
    /// Check if the client is connected
    pub fn is_connected(&self) -> bool {
        self.connected
    }
    
    /// Get the endpoint this client connects to
    pub fn endpoint(&self) -> SocketAddr {
        self.endpoint
    }
}

#[async_trait]
impl Client for TestClient {
    async fn connect(endpoint: SocketAddr) -> Result<Self> {
        info!("Connecting test client to {}", endpoint);
        
        // TODO: Implement actual WebSocket connection
        // For now, we'll just simulate a connection
        
        Ok(Self {
            endpoint,
            connected: true,
        })
    }
    
    async fn action(&self, name: &str, params: Value) -> Result<Value> {
        if !self.connected {
            return Err(Error::client("Client not connected"));
        }
        
        debug!("Invoking action '{}' with params: {}", name, params);
        
        // TODO: Implement actual WebSocket communication
        // For now, return a placeholder response
        Ok(serde_json::json!({
            "action": name,
            "params": params,
            "status": "not_implemented",
            "message": "Action invocation not yet implemented"
        }))
    }
    
    async fn list_actions(&self) -> Result<Vec<Value>> {
        if !self.connected {
            return Err(Error::client("Client not connected"));
        }
        
        debug!("Listing available actions");
        
        // TODO: Implement actual action discovery
        Ok(vec![serde_json::json!({
            "name": "example",
            "description": "Example action",
            "category": "test"
        })])
    }
    
    async fn disconnect(&self) -> Result<()> {
        info!("Disconnecting test client from {}", self.endpoint);
        
        // TODO: Implement actual WebSocket disconnection
        
        Ok(())
    }
}

/// Helper functions for testing
impl TestClient {
    /// Wait for a condition to be true (polling-based)
    pub async fn wait_for<F, Fut>(&self, condition: F, timeout_ms: u64) -> Result<()>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);
        
        while start.elapsed() < timeout {
            if condition().await {
                return Ok(());
            }
            
            // Yield control to allow other tasks to run
            futures::future::ready(()).await;
        }
        
        Err(Error::client(format!(
            "Timeout after {}ms waiting for condition",
            timeout_ms
        )))
    }
    
    /// Wait for a service to be in a specific state
    pub async fn wait_for_service_state(
        &self,
        service_name: &str,
        expected_state: &str,
        timeout_ms: u64,
    ) -> Result<()> {
        self.wait_for(
            || async {
                // TODO: Query service status
                // For now, just return false to simulate waiting
                false
            },
            timeout_ms,
        )
        .await
    }
    
    /// Wait for all services to be healthy
    pub async fn wait_for_healthy(&self, timeout_ms: u64) -> Result<()> {
        self.wait_for(
            || async {
                // TODO: Check all service health
                false
            },
            timeout_ms,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_client_creation() {
        let endpoint = "127.0.0.1:9443".parse().unwrap();
        let client = TestClient::new(endpoint);
        
        assert_eq!(client.endpoint(), endpoint);
        assert!(!client.is_connected());
    }
    
    #[test]
    fn test_client_connection() {
        smol::block_on(async {
            let endpoint = "127.0.0.1:9443".parse().unwrap();
            let client = TestClient::connect(endpoint).await.unwrap();
            
            assert!(client.is_connected());
            assert_eq!(client.endpoint(), endpoint);
        });
    }
    
    #[test]
    fn test_action_invocation_not_connected() {
        smol::block_on(async {
            let endpoint = "127.0.0.1:9443".parse().unwrap();
            let client = TestClient::new(endpoint);
            
            let result = client.action("test", json!({})).await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("not connected"));
        });
    }
    
    #[test]
    fn test_wait_for_timeout() {
        smol::block_on(async {
            let endpoint = "127.0.0.1:9443".parse().unwrap();
            let client = TestClient::connect(endpoint).await.unwrap();
            
            let result = client
                .wait_for(|| async { false }, 100) // Always false, should timeout
                .await;
            
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("Timeout"));
        });
    }
}