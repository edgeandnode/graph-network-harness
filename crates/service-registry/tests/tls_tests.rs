//! TLS integration tests for WebSocket connections

#[cfg(all(test, feature = "integration-tests"))]
mod tls_tests {

    use rcgen::generate_simple_self_signed;
    use service_registry::{Registry, TlsClientConfig, TlsServerConfig, WsClient, WsServer};
    use std::time::Duration;

    /// Helper to create test certificates
    async fn create_test_certificates()
    -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let subject_alt_names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
        let cert = generate_simple_self_signed(subject_alt_names)
            .expect("Failed to generate self-signed certificate");

        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let cert_path = temp_dir.path().join("test-cert.pem");
        let key_path = temp_dir.path().join("test-key.pem");

        // Write certificate
        async_fs::write(
            &cert_path,
            cert.serialize_pem().expect("Failed to serialize cert"),
        )
        .await
        .expect("Failed to write certificate");

        // Write key
        async_fs::write(&key_path, cert.serialize_private_key_pem())
            .await
            .expect("Failed to write key");

        (temp_dir, cert_path, key_path)
    }

    /// Helper to create a client config that accepts any certificate
    fn create_test_client_config() -> TlsClientConfig {
        use rustls::client::danger::{
            HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
        };
        use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
        use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
        use std::sync::Arc;

        #[derive(Debug)]
        struct NoVerifier;

        impl ServerCertVerifier for NoVerifier {
            fn verify_server_cert(
                &self,
                _end_entity: &CertificateDer<'_>,
                _intermediates: &[CertificateDer<'_>],
                _server_name: &ServerName<'_>,
                _ocsp_response: &[u8],
                _now: UnixTime,
            ) -> Result<ServerCertVerified, rustls::Error> {
                Ok(ServerCertVerified::assertion())
            }

            fn verify_tls12_signature(
                &self,
                _message: &[u8],
                _cert: &CertificateDer<'_>,
                _dss: &DigitallySignedStruct,
            ) -> Result<HandshakeSignatureValid, rustls::Error> {
                Ok(HandshakeSignatureValid::assertion())
            }

            fn verify_tls13_signature(
                &self,
                _message: &[u8],
                _cert: &CertificateDer<'_>,
                _dss: &DigitallySignedStruct,
            ) -> Result<HandshakeSignatureValid, rustls::Error> {
                Ok(HandshakeSignatureValid::assertion())
            }

            fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
                vec![
                    SignatureScheme::RSA_PKCS1_SHA256,
                    SignatureScheme::RSA_PKCS1_SHA384,
                    SignatureScheme::RSA_PKCS1_SHA512,
                    SignatureScheme::ECDSA_NISTP256_SHA256,
                    SignatureScheme::ECDSA_NISTP384_SHA384,
                    SignatureScheme::RSA_PSS_SHA256,
                    SignatureScheme::RSA_PSS_SHA384,
                    SignatureScheme::RSA_PSS_SHA512,
                    SignatureScheme::ED25519,
                ]
            }
        }

        let config = ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerifier))
            .with_no_client_auth();

        TlsClientConfig {
            config: Arc::new(config),
        }
    }

    /// Test TLS WebSocket connection with self-signed certificate
    #[smol_potat::test]
    async fn test_tls_websocket_connection() {
        // Create test certificates
        let (_temp_dir, cert_path, key_path) = create_test_certificates().await;

        // Create TLS server config
        let tls_config = TlsServerConfig::from_files(&cert_path, &key_path)
            .await
            .expect("Failed to create TLS config");

        // Start TLS server
        let registry = Registry::new().await;
        let server = WsServer::new_tls("127.0.0.1:0", registry, tls_config)
            .await
            .expect("Failed to create TLS server");

        let server_addr = server
            .listener
            .local_addr()
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

        let client = WsClient::connect_tls(server_addr, client_tls_config, "localhost")
            .await
            .expect("Failed to connect TLS client");

        let (handle, handler) = client.start_handler().await;

        // Run handler in background
        let handler_task = smol::spawn(handler);

        // Test basic operations over TLS
        let services = handle
            .list_services()
            .await
            .expect("Failed to list services");
        assert_eq!(services.len(), 0);

        let endpoints = handle
            .list_endpoints()
            .await
            .expect("Failed to list endpoints");
        assert!(endpoints.is_empty());

        // Clean up
        handle.close().await.expect("Failed to close client");
        drop(handler_task);
        drop(server_task);
    }

    /// Test TLS with proper certificate validation
    #[smol_potat::test]
    async fn test_tls_certificate_validation() {
        // Create self-signed certificate
        let subject_alt_names = vec!["testserver.local".to_string()];
        let cert =
            generate_simple_self_signed(subject_alt_names).expect("Failed to generate certificate");

        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let cert_path = temp_dir.path().join("server-cert.pem");
        let key_path = temp_dir.path().join("server-key.pem");
        let ca_cert_path = temp_dir.path().join("ca-cert.pem");

        // Write certificate files
        async_fs::write(&cert_path, cert.serialize_pem().unwrap())
            .await
            .unwrap();
        async_fs::write(&key_path, cert.serialize_private_key_pem())
            .await
            .unwrap();
        async_fs::write(&ca_cert_path, cert.serialize_pem().unwrap())
            .await
            .unwrap(); // Self-signed acts as CA

        // Create server with TLS
        let tls_config = TlsServerConfig::from_files(&cert_path, &key_path)
            .await
            .expect("Failed to create TLS config");

        let registry = Registry::new().await;
        let server = WsServer::new_tls("127.0.0.1:0", registry, tls_config)
            .await
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
        let default_client_config =
            TlsClientConfig::default().expect("Failed to create default client config");

        let result =
            WsClient::connect_tls(server_addr, default_client_config, "testserver.local").await;
        assert!(result.is_err(), "Should fail without CA certificate");

        // Test 2: Connection should succeed with CA certificate
        let client_config_with_ca = TlsClientConfig::with_ca_cert(&ca_cert_path)
            .await
            .expect("Failed to create client config with CA");

        let client = WsClient::connect_tls(server_addr, client_config_with_ca, "testserver.local")
            .await
            .expect("Failed to connect with CA cert");

        let (handle, handler) = client.start_handler().await;
        let _handler_task = smol::spawn(handler);

        // Verify connection works
        let services = handle
            .list_services()
            .await
            .expect("Failed to list services");
        assert_eq!(services.len(), 0);

        handle.close().await.unwrap();
    }

    /// Test mixed plain and TLS connections
    #[smol_potat::test]
    async fn test_mixed_plain_and_tls_connections() {
        // Create separate registries for each server since Registry doesn't implement Clone
        let plain_registry = Registry::new().await;
        let tls_registry = Registry::new().await;

        // Start plain WebSocket server
        let plain_server = WsServer::new("127.0.0.1:0", plain_registry)
            .await
            .expect("Failed to create plain server");
        let plain_addr = plain_server.listener.local_addr().unwrap();

        // Create test certificates and TLS config
        let (_temp_dir, cert_path, key_path) = create_test_certificates().await;
        let tls_config = TlsServerConfig::from_files(&cert_path, &key_path)
            .await
            .expect("Failed to create TLS config");

        // Start TLS WebSocket server
        let tls_server = WsServer::new_tls("127.0.0.1:0", tls_registry, tls_config)
            .await
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
        let plain_client = WsClient::connect(plain_addr)
            .await
            .expect("Failed to connect plain client");
        let (plain_handle, plain_handler) = plain_client.start_handler().await;
        let _plain_handler = smol::spawn(plain_handler);

        // Connect TLS client to TLS server
        let tls_client_config = create_test_client_config();
        let tls_client = WsClient::connect_tls(tls_addr, tls_client_config, "localhost")
            .await
            .expect("Failed to connect TLS client");
        let (tls_handle, tls_handler) = tls_client.start_handler().await;
        let _tls_handler = smol::spawn(tls_handler);

        // Test both connections work
        let plain_services = plain_handle
            .list_services()
            .await
            .expect("Plain client failed");
        let tls_services = tls_handle.list_services().await.expect("TLS client failed");

        assert_eq!(plain_services.len(), tls_services.len());

        // Clean up
        plain_handle.close().await.unwrap();
        tls_handle.close().await.unwrap();
    }
}

// Export test module when integration tests are enabled
