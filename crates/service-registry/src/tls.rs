//! TLS configuration and utilities

use crate::error::{Error, Result};
use std::path::Path;
use std::sync::Arc;

use rustls::{Certificate, PrivateKey, ServerConfig, ClientConfig};

/// TLS configuration for server
#[derive(Clone)]
pub struct TlsServerConfig {
    pub config: Arc<ServerConfig>,
}

/// TLS configuration for client
#[derive(Clone)]
pub struct TlsClientConfig {
    pub config: Arc<ClientConfig>,
}

impl TlsServerConfig {
    /// Create TLS server configuration from certificate and key files
    pub async fn from_files(cert_path: impl AsRef<Path>, key_path: impl AsRef<Path>) -> Result<Self> {
        use async_fs::File;
        use futures::io::AsyncReadExt;
        
        // Read certificate file
        let mut cert_file = File::open(cert_path.as_ref()).await
            .map_err(|e| Error::Package(format!("Failed to open certificate file: {}", e)))?;
        let mut cert_bytes = Vec::new();
        cert_file.read_to_end(&mut cert_bytes).await
            .map_err(|e| Error::Package(format!("Failed to read certificate: {}", e)))?;
        
        // Read key file
        let mut key_file = File::open(key_path.as_ref()).await
            .map_err(|e| Error::Package(format!("Failed to open key file: {}", e)))?;
        let mut key_bytes = Vec::new();
        key_file.read_to_end(&mut key_bytes).await
            .map_err(|e| Error::Package(format!("Failed to read key: {}", e)))?;
        
        // Parse certificates
        let certs = rustls_pemfile::certs(&mut cert_bytes.as_slice())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Package(format!("Failed to parse certificates: {}", e)))?
            .into_iter()
            .map(|der| Certificate(der.to_vec()))
            .collect::<Vec<_>>();
        
        if certs.is_empty() {
            return Err(Error::Package("No certificates found in file".to_string()));
        }
        
        // Parse private key
        let key_der = rustls_pemfile::private_key(&mut key_bytes.as_slice())
            .map_err(|e| Error::Package(format!("Failed to parse private key: {}", e)))?
            .ok_or_else(|| Error::Package("No private key found in file".to_string()))?;
        
        let key = PrivateKey(key_der.secret_der().to_vec());
        
        // Build server config
        let config = ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| Error::Package(format!("Failed to create TLS config: {}", e)))?;
        
        Ok(Self {
            config: Arc::new(config),
        })
    }
    
    /// Create TLS server configuration for testing with self-signed certificate
    #[cfg(test)]
    pub fn self_signed_for_testing() -> Result<Self> {
        use rcgen::generate_simple_self_signed;
        
        let subject_alt_names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
        let cert = generate_simple_self_signed(subject_alt_names)
            .map_err(|e| Error::Package(format!("Failed to generate self-signed cert: {}", e)))?;
        
        let cert_der = cert.serialize_der()
            .map_err(|e| Error::Package(format!("Failed to serialize cert: {}", e)))?;
        let key_der = cert.serialize_private_key_der();
        
        let certs = vec![Certificate(cert_der)];
        let key = PrivateKey(key_der);
        
        let config = ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| Error::Package(format!("Failed to create TLS config: {}", e)))?;
        
        Ok(Self {
            config: Arc::new(config),
        })
    }
    
}

impl TlsClientConfig {
    /// Create TLS client configuration with default settings (uses system root certificates)
    pub fn default() -> Result<Self> {
        let mut root_store = rustls::RootCertStore::empty();
        root_store.add_trust_anchors(
            webpki_roots::TLS_SERVER_ROOTS.0.iter().map(|ta| {
                rustls::OwnedTrustAnchor::from_subject_spki_name_constraints(
                    ta.subject,
                    ta.spki,
                    ta.name_constraints,
                )
            })
        );
        
        let config = ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        
        Ok(Self {
            config: Arc::new(config),
        })
    }
    
    /// Create TLS client configuration that accepts self-signed certificates (for testing)
    #[cfg(test)]
    pub fn dangerous_accept_any_cert() -> Result<Self> {
        use rustls::client::ServerCertVerifier;
        
        // Custom verifier that accepts any certificate
        struct DangerousAcceptAnyVerifier;
        
        impl ServerCertVerifier for DangerousAcceptAnyVerifier {
            fn verify_server_cert(
                &self,
                _end_entity: &Certificate,
                _intermediates: &[Certificate],
                _server_name: &rustls::ServerName,
                _scts: &mut dyn Iterator<Item = &[u8]>,
                _ocsp_response: &[u8],
                _now: std::time::SystemTime,
            ) -> std::result::Result<rustls::client::ServerCertVerified, rustls::Error> {
                Ok(rustls::client::ServerCertVerified::assertion())
            }
        }
        
        let config = ClientConfig::builder()
            .with_safe_defaults()
            .with_custom_certificate_verifier(Arc::new(DangerousAcceptAnyVerifier))
            .with_no_client_auth();
        
        Ok(Self {
            config: Arc::new(config),
        })
    }
    
    /// Create TLS client configuration with custom CA certificate
    pub async fn with_ca_cert(ca_cert_path: impl AsRef<Path>) -> Result<Self> {
        use async_fs::File;
        use futures::io::AsyncReadExt;
        
        // Read CA certificate
        let mut ca_file = File::open(ca_cert_path.as_ref()).await
            .map_err(|e| Error::Package(format!("Failed to open CA certificate file: {}", e)))?;
        let mut ca_bytes = Vec::new();
        ca_file.read_to_end(&mut ca_bytes).await
            .map_err(|e| Error::Package(format!("Failed to read CA certificate: {}", e)))?;
        
        // Parse CA certificates
        let ca_certs = rustls_pemfile::certs(&mut ca_bytes.as_slice())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Package(format!("Failed to parse CA certificates: {}", e)))?;
        
        let mut root_store = rustls::RootCertStore::empty();
        for cert in ca_certs {
            root_store.add(&Certificate(cert.to_vec()))
                .map_err(|e| Error::Package(format!("Failed to add CA certificate: {}", e)))?;
        }
        
        let config = ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        
        Ok(Self {
            config: Arc::new(config),
        })
    }
}


/// TLS acceptor for server
pub type TlsAcceptor = async_tls::TlsAcceptor;

/// TLS connector for client
pub type TlsConnector = async_tls::TlsConnector;