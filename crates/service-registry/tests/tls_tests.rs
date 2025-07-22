//! TLS integration tests for WebSocket connections

#[cfg(all(test, feature = "tls", feature = "integration-tests"))]
mod tls_tests {
    use service_registry::{Registry, WsServer, WsClient, TlsServerConfig, TlsClientConfig};
    use std::time::Duration;
    use rcgen::{generate_simple_self_signed, CertifiedKey};
    use async_fs;
    use tempfile;
    
    /// Helper to create test certificates
    async fn create_test_certificates() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let subject_alt_names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
        let CertifiedKey { cert, key_pair } = generate_simple_self_signed(subject_alt_names)
            .expect("Failed to generate self-signed certificate");
        
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let cert_path = temp_dir.path().join("test-cert.pem");
        let key_path = temp_dir.path().join("test-key.pem");
        
        // Write certificate
        async_fs::write(&cert_path, cert.pem()).await
            .expect("Failed to write certificate");
        
        // Write key
        async_fs::write(&key_path, key_pair.serialize_pem()).await
            .expect("Failed to write key");
        
        (temp_dir, cert_path, key_path)
    }
    
    /// Helper to create a client config that accepts any certificate
    fn create_test_client_config() -> TlsClientConfig {
        use rustls::{Certificate, ClientConfig};
        use rustls::client::ServerCertVerifier;
        use std::sync::Arc;
        
        struct NoVerifier;
        
        impl ServerCertVerifier for NoVerifier {
            fn verify_server_cert(
                &self,
                _end_entity: &Certificate,
                _intermediates: &[Certificate],
                _server_name: &rustls::ServerName,
                _scts: &mut dyn Iterator<Item = &[u8]>,
                _ocsp_response: &[u8],
                _now: std::time::SystemTime,
            ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
                Ok(rustls::client::ServerCertVerified::assertion())
            }
        }
        
        let config = ClientConfig::builder()
            .with_safe_defaults()
            .with_custom_certificate_verifier(Arc::new(NoVerifier))
            .with_no_client_auth();
        
        TlsClientConfig { config: Arc::new(config) }
    }
    
    /// Test TLS WebSocket connection with self-signed certificate
    #[test]
    fn test_tls_websocket_connection() {
        smol::block_on(async {
            // Create test certificates
            let (_temp_dir, cert_path, key_path) = create_test_certificates().await;
            
            // Create TLS server config
            let tls_config = TlsServerConfig::from_files(&cert_path, &key_path).await
                .expect("Failed to create TLS config");
            
            // Start TLS server
            let registry = Registry::new();
            let server = WsServer::new_tls("127.0.0.1:0", registry, tls_config).await
                .expect("Failed to create TLS server");
            
            let server_addr = server.listener.local_addr()
                .expect("Failed to get server address");
            
            // Run server in background
            let server_task = smol::spawn(async move {
                loop {
                    match server.accept().await {
                        Ok(handler) => {
                            smol::spawn(handler.handle()).detach();
                        }
                        Err(e) => {
                            eprintln!("Accept error: {}", e);
                            break;
                        }
                    }
                }
            });
            
            // Give server time to start
            smol::Timer::after(Duration::from_millis(100)).await;
            
            // Create client with test certificate acceptance
            let client_tls_config = create_test_client_config();
            
            let client = WsClient::connect_tls(server_addr, client_tls_config, "localhost").await
                .expect("Failed to connect TLS client");
            
            let (handle, handler) = client.start_handler().await;
            
            // Run handler in background
            let handler_task = smol::spawn(handler);
            
            // Test basic operations over TLS
            let services = handle.list_services().await
                .expect("Failed to list services");
            assert_eq!(services.len(), 0);
            
            let endpoints = handle.list_endpoints().await
                .expect("Failed to list endpoints");
            assert!(endpoints.is_empty());
            
            // Clean up
            handle.close().await.expect("Failed to close client");
            drop(handler_task);
            drop(server_task);
        });
    }
    
    /// Test TLS with proper certificate validation
    #[test]
    fn test_tls_certificate_validation() {
        smol::block_on(async {
            // Create self-signed certificate
            let subject_alt_names = vec!["testserver.local".to_string()];
            let CertifiedKey { cert, key_pair } = generate_simple_self_signed(subject_alt_names)
                .expect("Failed to generate certificate");
            
            let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
            let cert_path = temp_dir.path().join("server-cert.pem");
            let key_path = temp_dir.path().join("server-key.pem");
            let ca_cert_path = temp_dir.path().join("ca-cert.pem");
            
            // Write certificate files
            async_fs::write(&cert_path, cert.pem()).await.unwrap();
            async_fs::write(&key_path, key_pair.serialize_pem()).await.unwrap();
            async_fs::write(&ca_cert_path, cert.pem()).await.unwrap(); // Self-signed acts as CA
            
            // Create server with TLS
            let tls_config = TlsServerConfig::from_files(&cert_path, &key_path).await
                .expect("Failed to create TLS config");
            
            let registry = Registry::new();
            let server = WsServer::new_tls("127.0.0.1:0", registry, tls_config).await
                .expect("Failed to create TLS server");
            
            let server_addr = server.listener.local_addr().unwrap();
            
            // Run server
            let _server_task = smol::spawn(async move {
                loop {
                    match server.accept().await {
                        Ok(handler) => {
                            smol::spawn(handler.handle()).detach();
                        }
                        Err(_) => break,
                    }
                }
            });
            
            smol::Timer::after(Duration::from_millis(100)).await;
            
            // Test 1: Connection should fail with default client config (no CA cert)
            let default_client_config = TlsClientConfig::default()
                .expect("Failed to create default client config");
            
            let result = WsClient::connect_tls(server_addr, default_client_config, "testserver.local").await;
            assert!(result.is_err(), "Should fail without CA certificate");
            
            // Test 2: Connection should succeed with CA certificate
            let client_config_with_ca = TlsClientConfig::with_ca_cert(&ca_cert_path).await
                .expect("Failed to create client config with CA");
            
            let client = WsClient::connect_tls(server_addr, client_config_with_ca, "testserver.local").await
                .expect("Failed to connect with CA cert");
            
            let (handle, handler) = client.start_handler().await;
            let _handler_task = smol::spawn(handler);
            
            // Verify connection works
            let services = handle.list_services().await
                .expect("Failed to list services");
            assert_eq!(services.len(), 0);
            
            handle.close().await.unwrap();
        });
    }
    
    /// Test mixed plain and TLS connections
    #[test]
    fn test_mixed_plain_and_tls_connections() {
        smol::block_on(async {
            let registry = Registry::new();
            
            // Start plain WebSocket server
            let plain_server = WsServer::new("127.0.0.1:0", registry.clone()).await
                .expect("Failed to create plain server");
            let plain_addr = plain_server.listener.local_addr().unwrap();
            
            // Create test certificates and TLS config
            let (_temp_dir, cert_path, key_path) = create_test_certificates().await;
            let tls_config = TlsServerConfig::from_files(&cert_path, &key_path).await
                .expect("Failed to create TLS config");
            
            // Start TLS WebSocket server
            let tls_server = WsServer::new_tls("127.0.0.1:0", registry, tls_config).await
                .expect("Failed to create TLS server");
            let tls_addr = tls_server.listener.local_addr().unwrap();
            
            // Run both servers
            let _plain_task = smol::spawn(async move {
                loop {
                    match plain_server.accept().await {
                        Ok(handler) => {
                            smol::spawn(handler.handle()).detach();
                        }
                        Err(_) => break,
                    }
                }
            });
            
            let _tls_task = smol::spawn(async move {
                loop {
                    match tls_server.accept().await {
                        Ok(handler) => {
                            smol::spawn(handler.handle()).detach();
                        }
                        Err(_) => break,
                    }
                }
            });
            
            smol::Timer::after(Duration::from_millis(100)).await;
            
            // Connect plain client to plain server
            let plain_client = WsClient::connect(plain_addr).await
                .expect("Failed to connect plain client");
            let (plain_handle, plain_handler) = plain_client.start_handler().await;
            let _plain_handler = smol::spawn(plain_handler);
            
            // Connect TLS client to TLS server
            let tls_client_config = create_test_client_config();
            let tls_client = WsClient::connect_tls(tls_addr, tls_client_config, "localhost").await
                .expect("Failed to connect TLS client");
            let (tls_handle, tls_handler) = tls_client.start_handler().await;
            let _tls_handler = smol::spawn(tls_handler);
            
            // Test both connections work
            let plain_services = plain_handle.list_services().await
                .expect("Plain client failed");
            let tls_services = tls_handle.list_services().await
                .expect("TLS client failed");
            
            assert_eq!(plain_services.len(), tls_services.len());
            
            // Clean up
            plain_handle.close().await.unwrap();
            tls_handle.close().await.unwrap();
        });
    }
}

// Export test module when both features are enabled
#[cfg(all(feature = "tls", feature = "integration-tests"))]
pub use tls_tests::*;