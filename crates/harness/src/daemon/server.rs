//! WebSocket server for the executor daemon

use crate::daemon::handlers;
use crate::protocol::{Request, Response};
use anyhow::{Context, Result};
use async_net::{TcpListener, TcpStream};
use async_tungstenite::accept_async;
use async_tungstenite::tungstenite::Message;
use futures::StreamExt;
use futures_rustls::TlsAcceptor;
use rustls::ServerConfig;
use rustls::pki_types::PrivateKeyDer;
use service_orchestration::ServiceManager;
use service_registry::Registry;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, error, info};

/// Daemon state shared between connections
///
/// Both ServiceManager and Registry implement their own internal synchronization:
/// - ServiceManager uses RwLock for active_services and health_monitors
/// - Registry uses Arc<Mutex<>> internally for its store and subscribers
/// Therefore, no external mutex is needed here.
pub struct DaemonState {
    pub service_manager: Arc<ServiceManager>,
    pub registry: Arc<Registry>,
}

/// Start the WebSocket server
pub async fn start_server(data_dir: &Path, port: u16) -> Result<()> {
    // Create service manager with persistent registry
    let service_manager = ServiceManager::new()
        .await
        .context("Failed to create service manager")?;

    // Create in-memory registry
    let registry = Registry::new().await;

    // Create daemon state
    let state = Arc::new(DaemonState {
        service_manager: Arc::new(service_manager),
        registry: Arc::new(registry),
    });

    // Load TLS configuration
    let cert_path = data_dir.join("certs/server.crt");
    let key_path = data_dir.join("certs/server.key");

    let cert_pem = fs::read_to_string(&cert_path).context("Failed to read certificate")?;
    let key_pem = fs::read_to_string(&key_path).context("Failed to read private key")?;

    // Parse certificate and key
    let certs = rustls_pemfile::certs(&mut cert_pem.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("Failed to parse certificate: {:?}", e))?;

    let key_der = rustls_pemfile::private_key(&mut key_pem.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to parse private key: {:?}", e))?
        .ok_or_else(|| anyhow::anyhow!("No private keys found in key file"))?;

    let key = PrivateKeyDer::try_from(key_der)
        .map_err(|e| anyhow::anyhow!("Failed to convert private key: {:?}", e))?;

    // Create TLS config
    let tls_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("Failed to create TLS config")?;

    let tls_acceptor = TlsAcceptor::from(Arc::new(tls_config));

    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr)
        .await
        .context("Failed to bind to address")?;

    info!("Executor daemon listening on wss://{}", addr);

    // Set up Ctrl+C handler
    let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();

    // Note: We can't use signal handling without tokio/async-std specific features
    // For now, the daemon will need to be stopped with Ctrl+C or kill
    // In production, it would be managed by systemd which handles signals properly

    // Accept connections
    loop {
        match listener.accept().await {
            Ok((stream, peer_addr)) => {
                debug!("New connection from {}", peer_addr);
                let state = state.clone();
                let tls_acceptor = tls_acceptor.clone();

                // Spawn handler task
                smol::spawn(async move {
                    // Accept TLS connection
                    match tls_acceptor.accept(stream).await {
                        Ok(tls_stream) => {
                            if let Err(e) = handle_connection(tls_stream, state).await {
                                error!("Connection handler error: {}", e);
                            }
                        }
                        Err(e) => {
                            error!("TLS handshake failed: {}", e);
                        }
                    }
                })
                .detach();
            }
            Err(e) => {
                error!("Failed to accept connection: {}", e);
                // Continue accepting other connections
            }
        }
    }
}

/// Handle a WebSocket connection
async fn handle_connection(
    stream: futures_rustls::server::TlsStream<TcpStream>,
    state: Arc<DaemonState>,
) -> Result<()> {
    let ws_stream = accept_async(stream)
        .await
        .context("Failed to accept WebSocket connection")?;

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                // Parse request
                let request: Request = match serde_json::from_str(&text) {
                    Ok(req) => req,
                    Err(e) => {
                        error!("Failed to parse request: {}", e);
                        let error_response = Response::Error {
                            message: format!("Invalid request format: {}", e),
                        };
                        let response_text = serde_json::to_string(&error_response)?;
                        ws_sender.send(Message::Text(response_text.into())).await?;
                        continue;
                    }
                };

                // Handle request
                let response = handlers::handle_request(request, state.clone()).await?;

                // Send response
                let response_text = serde_json::to_string(&response)?;
                ws_sender.send(Message::Text(response_text.into())).await?;
            }
            Ok(Message::Close(_)) => {
                debug!("Client requested close");
                break;
            }
            Ok(_) => {
                // Ignore other message types
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
        }
    }

    debug!("Connection closed");
    Ok(())
}
