//! WebSocket client for communicating with the daemon

use anyhow::{anyhow, Context, Result};
use async_net::TcpStream;
use futures_rustls::{client::TlsStream, TlsConnector};
use async_tungstenite::tungstenite::Message;
use async_tungstenite::{client_async, WebSocketStream};
use futures::StreamExt;
use rustls::{ClientConfig, RootCertStore};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::debug;

/// Daemon client for sending requests
pub enum DaemonClient {
    Plain(WebSocketStream<TcpStream>),
    Tls(WebSocketStream<TlsStream<TcpStream>>),
}

impl DaemonClient {
    /// Connect to the daemon (without TLS)
    pub async fn connect(port: u16) -> Result<Self> {
        let addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;
        let url = format!("ws://{}/", addr);

        let stream = TcpStream::connect(addr)
            .await
            .context("Failed to connect to daemon")?;

        let (ws, _) = client_async(&url, stream)
            .await
            .context("Failed to establish WebSocket connection")?;

        debug!("Connected to daemon at {} (no TLS)", addr);

        Ok(Self::Plain(ws))
    }

    /// Connect with TLS
    pub async fn connect_tls(port: u16, verify_cert: bool) -> Result<Self> {
        let addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;
        let url = format!("wss://{}/", addr);

        // Load the daemon's certificate
        let cert_path = dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("harness/certs/server.crt");

        if !cert_path.exists() {
            return Err(anyhow!(
                "Daemon certificate not found at {:?}. Has the daemon been started?",
                cert_path
            ));
        }

        // Read and parse the certificate
        let cert_pem =
            std::fs::read_to_string(&cert_path).context("Failed to read daemon certificate")?;

        let certs = rustls_pemfile::certs(&mut cert_pem.as_bytes())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("Failed to parse certificate: {:?}", e))?;

        if certs.is_empty() {
            return Err(anyhow!("No certificates found in {:?}", cert_path));
        }

        // Create root certificate store with our daemon's certificate
        let mut root_store = RootCertStore::empty();
        for cert in certs {
            root_store
                .add(cert)
                .map_err(|e| anyhow!("Failed to add certificate to root store: {:?}", e))?;
        }

        // Create TLS config that trusts our specific certificate
        let config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let tls_connector = TlsConnector::from(Arc::new(config));

        let tcp_stream = TcpStream::connect(addr).await?;

        let server_name = rustls::pki_types::ServerName::try_from("localhost")
            .map_err(|e| anyhow!("Invalid server name: {:?}", e))?;
        let tls_stream = tls_connector
            .connect(server_name, tcp_stream)
            .await
            .context("TLS handshake failed")?;

        let (ws, _) = client_async(&url, tls_stream)
            .await
            .context("Failed to establish WebSocket connection")?;

        debug!("Connected to daemon at {} (TLS)", addr);

        Ok(Self::Tls(ws))
    }

    /// Send a request to the daemon and get response
    pub async fn send_request(
        &mut self,
        request: crate::protocol::Request,
    ) -> Result<crate::protocol::Response> {
        // Serialize request
        let request_json = serde_json::to_string(&request)?;

        // Send request
        match self {
            Self::Plain(ws) => ws.send(Message::Text(request_json.into())).await?,
            Self::Tls(ws) => ws.send(Message::Text(request_json.into())).await?,
        }

        // Wait for response
        let msg = match self {
            Self::Plain(ws) => ws.next().await,
            Self::Tls(ws) => ws.next().await,
        };

        match msg {
            Some(Ok(Message::Text(text))) => {
                let response: crate::protocol::Response =
                    serde_json::from_str(&text).context("Failed to parse daemon response")?;
                Ok(response)
            }
            Some(Ok(Message::Close(_))) => Err(anyhow!("Connection closed by daemon")),
            Some(Err(e)) => Err(anyhow!("WebSocket error: {}", e)),
            None => Err(anyhow!("Connection closed unexpectedly")),
            _ => Err(anyhow!("Unexpected message type from daemon")),
        }
    }

    /// Close the connection
    pub async fn close(&mut self) -> Result<()> {
        match self {
            Self::Plain(ws) => ws.close(None).await?,
            Self::Tls(ws) => ws.close(None).await?,
        }
        Ok(())
    }
}
