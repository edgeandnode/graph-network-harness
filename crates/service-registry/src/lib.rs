//! Runtime-agnostic service registry with WebSocket API
//! 
//! This crate provides a service registry for tracking distributed services
//! and their endpoints. It uses a WebSocket-only API for all interactions,
//! enabling real-time monitoring and control.
//! 
//! # Architecture
//! 
//! The registry is designed to be runtime-agnostic, working with any async
//! runtime (tokio, async-std, smol, etc). It uses:
//! 
//! - `async-tungstenite` for WebSocket support (without runtime features)
//! - `async-net` for networking
//! - `async-fs` for file persistence
//! - Standard `futures` traits
//! 
//! # Example
//! 
//! ```no_run
//! use service_registry::{Registry, WsServer};
//! 
//! # async fn example() -> anyhow::Result<()> {
//! // Create registry
//! let registry = Registry::new();
//! 
//! // Create WebSocket server
//! let server = WsServer::new("127.0.0.1:8080", registry).await?;
//! 
//! // Accept connections - runtime agnostic
//! loop {
//!     let handler = server.accept().await?;
//!     // User chooses how to run the handler
//!     // e.g., tokio::spawn, smol::spawn, etc.
//! }
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

pub mod error;
pub mod models;
pub mod registry;
pub mod websocket;
pub mod package;
pub mod client;
pub mod tls;
pub mod config;
pub mod network;

pub use error::{Error, Result};
pub use models::*;
pub use registry::Registry;
pub use websocket::{WsServer, ConnectionHandler};
pub use package::{Package, PackageBuilder};
pub use client::{WsClient, WsClientHandle};
pub use tls::{TlsServerConfig, TlsClientConfig};
pub use config::{RegistryConfig, ServerConfig, ClientConfig, TlsConfig, ClientTlsConfig};

/// Re-export key types for convenience
pub mod prelude {
    pub use crate::{
        Registry,
        WsServer,
        ServiceEntry,
        ServiceState,
        Location,
        ExecutionInfo,
        Endpoint,
        Error,
        Result,
    };
}