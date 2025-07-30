//! TLS configuration and utilities

use crate::error::{Error, Result};
use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ClientConfig, ServerConfig};

/// TLS configuration for server
#[derive(Clone)]
pub struct TlsServerConfig {
    /// The underlying rustls server configuration
    pub config: Arc<ServerConfig>,
}

/// TLS configuration for client
#[derive(Clone)]
pub struct TlsClientConfig {
    /// The underlying rustls client configuration
    pub config: Arc<ClientConfig>,
}

impl TlsServerConfig {
    /// Create TLS server configuration from certificate and key files
    pub async fn from_files(
        cert_path: impl AsRef<Path>,
        key_path: impl AsRef<Path>,
    ) -> Result<Self> {
        use async_fs::File;
        use futures::io::AsyncReadExt;

        // Read certificate file
        let mut cert_file = File::open(cert_path.as_ref())
            .await
            .map_err(|e| Error::Package(format!("Failed to open certificate file: {}", e)))?;
        let mut cert_bytes = Vec::new();
        cert_file
            .read_to_end(&mut cert_bytes)
            .await
            .map_err(|e| Error::Package(format!("Failed to read certificate: {}", e)))?;

        // Read key file
        let mut key_file = File::open(key_path.as_ref())
            .await
            .map_err(|e| Error::Package(format!("Failed to open key file: {}", e)))?;
        let mut key_bytes = Vec::new();
        key_file
            .read_to_end(&mut key_bytes)
            .await
            .map_err(|e| Error::Package(format!("Failed to read key: {}", e)))?;

        // Parse certificates
        let certs = rustls_pemfile::certs(&mut cert_bytes.as_slice())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Package(format!("Failed to parse certificates: {}", e)))?;

        if certs.is_empty() {
            return Err(Error::Package("No certificates found in file".to_string()));
        }

        // Parse private key
        let key_der = rustls_pemfile::private_key(&mut key_bytes.as_slice())
            .map_err(|e| Error::Package(format!("Failed to parse private key: {}", e)))?
            .ok_or_else(|| Error::Package("No private key found in file".to_string()))?;

        let key = key_der;

        // Build server config
        let config = ServerConfig::builder()
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

        let cert_der = cert
            .serialize_der()
            .map_err(|e| Error::Package(format!("Failed to serialize cert: {}", e)))?;
        let key_der = cert.serialize_private_key_der();

        let certs = vec![CertificateDer::from(cert_der)];
        let key = PrivateKeyDer::try_from(key_der)
            .map_err(|e| Error::Package(format!("Failed to convert private key: {:?}", e)))?;

        let config = ServerConfig::builder()
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
    pub fn new() -> Result<Self> {
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        let config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        Ok(Self {
            config: Arc::new(config),
        })
    }

    /// Create TLS client configuration that accepts self-signed certificates (for testing)
    #[cfg(test)]
    pub fn dangerous_accept_any_cert() -> Result<Self> {
        use rustls::DigitallySignedStruct;
        use rustls::client::danger::{
            HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
        };
        use rustls::pki_types::UnixTime;

        #[derive(Debug)]
        struct DangerousAcceptAnyVerifier;

        impl ServerCertVerifier for DangerousAcceptAnyVerifier {
            fn verify_server_cert(
                &self,
                _end_entity: &CertificateDer<'_>,
                _intermediates: &[CertificateDer<'_>],
                _server_name: &rustls::pki_types::ServerName<'_>,
                _ocsp_response: &[u8],
                _now: UnixTime,
            ) -> std::result::Result<ServerCertVerified, rustls::Error> {
                Ok(ServerCertVerified::assertion())
            }

            fn verify_tls12_signature(
                &self,
                _message: &[u8],
                _cert: &CertificateDer<'_>,
                _dss: &DigitallySignedStruct,
            ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
                Ok(HandshakeSignatureValid::assertion())
            }

            fn verify_tls13_signature(
                &self,
                _message: &[u8],
                _cert: &CertificateDer<'_>,
                _dss: &DigitallySignedStruct,
            ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
                Ok(HandshakeSignatureValid::assertion())
            }

            fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
                vec![
                    rustls::SignatureScheme::RSA_PKCS1_SHA256,
                    rustls::SignatureScheme::RSA_PKCS1_SHA384,
                    rustls::SignatureScheme::RSA_PKCS1_SHA512,
                    rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
                    rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
                    rustls::SignatureScheme::RSA_PSS_SHA256,
                    rustls::SignatureScheme::RSA_PSS_SHA384,
                    rustls::SignatureScheme::RSA_PSS_SHA512,
                    rustls::SignatureScheme::ED25519,
                ]
            }
        }

        let config = ClientConfig::builder()
            .dangerous()
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
        let mut ca_file = File::open(ca_cert_path.as_ref())
            .await
            .map_err(|e| Error::Package(format!("Failed to open CA certificate file: {}", e)))?;
        let mut ca_bytes = Vec::new();
        ca_file
            .read_to_end(&mut ca_bytes)
            .await
            .map_err(|e| Error::Package(format!("Failed to read CA certificate: {}", e)))?;

        // Parse CA certificates
        let ca_certs = rustls_pemfile::certs(&mut ca_bytes.as_slice())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Package(format!("Failed to parse CA certificates: {}", e)))?;

        let mut root_store = rustls::RootCertStore::empty();
        for cert in ca_certs {
            root_store
                .add(cert)
                .map_err(|e| Error::Package(format!("Failed to add CA certificate: {}", e)))?;
        }

        let config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        Ok(Self {
            config: Arc::new(config),
        })
    }
}

/// TLS acceptor for server
pub type TlsAcceptor = futures_rustls::TlsAcceptor;

/// TLS connector for client  
pub type TlsConnector = futures_rustls::TlsConnector;
