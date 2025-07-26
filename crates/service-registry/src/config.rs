//! Configuration structures for service registry

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Service registry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// Server configuration
    pub server: ServerConfig,
    /// Client configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<ClientConfig>,
    /// Package management configuration
    #[serde(default)]
    pub packages: PackageConfig,
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Listen address (e.g., "127.0.0.1:8080")
    pub listen_addr: String,
    /// TLS configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsConfig>,
}

/// Client configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    /// Default server address
    pub server_addr: String,
    /// TLS configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls: Option<ClientTlsConfig>,
}

/// TLS configuration for server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Path to certificate file (PEM format)
    pub cert_path: PathBuf,
    /// Path to private key file (PEM format)
    pub key_path: PathBuf,
    /// Whether to require client certificates
    #[serde(default)]
    pub require_client_cert: bool,
}

/// TLS configuration for client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientTlsConfig {
    /// Server name for certificate validation
    pub server_name: String,
    /// Path to CA certificate for validation (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ca_cert_path: Option<PathBuf>,
    /// Whether to accept invalid certificates (DANGEROUS - testing only)
    #[serde(default)]
    pub accept_invalid_certs: bool,
}

/// Package management configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PackageConfig {
    /// Base directory for package installations
    #[serde(default = "default_package_dir")]
    pub install_dir: PathBuf,
    /// Whether to enable package signature verification
    #[serde(default)]
    pub verify_signatures: bool,
}

fn default_package_dir() -> PathBuf {
    PathBuf::from("/opt")
}

impl RegistryConfig {
    /// Load configuration from file
    pub async fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self> {
        use async_fs::File;
        use futures::io::AsyncReadExt;

        let mut file = File::open(path.as_ref()).await?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).await?;

        // Try YAML first, then JSON
        if path.as_ref().extension().and_then(|s| s.to_str()) == Some("yaml")
            || path.as_ref().extension().and_then(|s| s.to_str()) == Some("yml")
        {
            Ok(serde_yaml::from_str(&contents)?)
        } else {
            Ok(serde_json::from_str(&contents)?)
        }
    }

    /// Create a default configuration
    pub fn default() -> Self {
        Self {
            server: ServerConfig {
                listen_addr: "127.0.0.1:8080".to_string(),
                tls: None,
            },
            client: None,
            packages: PackageConfig::default(),
        }
    }

    /// Create a configuration with TLS enabled
    pub fn with_tls(cert_path: PathBuf, key_path: PathBuf) -> Self {
        Self {
            server: ServerConfig {
                listen_addr: "127.0.0.1:8443".to_string(),
                tls: Some(TlsConfig {
                    cert_path,
                    key_path,
                    require_client_cert: false,
                }),
            },
            client: None,
            packages: PackageConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_serialization() {
        let config = RegistryConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        let parsed: RegistryConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.server.listen_addr, config.server.listen_addr);
    }

    #[test]
    fn test_tls_config() {
        let config = RegistryConfig::with_tls(
            PathBuf::from("/path/to/cert.pem"),
            PathBuf::from("/path/to/key.pem"),
        );
        assert!(config.server.tls.is_some());
        assert_eq!(config.server.listen_addr, "127.0.0.1:8443");
    }
}
