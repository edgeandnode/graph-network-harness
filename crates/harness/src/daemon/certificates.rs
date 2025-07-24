//! TLS certificate management for the daemon

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use std::fs;
use std::path::Path;
use tracing::{error, info, warn};

/// Check if certificates exist and are valid
pub async fn ensure_valid_certificates(data_dir: &Path, interactive: bool) -> Result<()> {
    let cert_dir = data_dir.join("certs");
    let cert_path = cert_dir.join("server.crt");
    let key_path = cert_dir.join("server.key");

    // Check if certificates exist
    if !cert_path.exists() || !key_path.exists() {
        info!("No TLS certificates found, generating new ones...");
        generate_certificates(&cert_dir)?;
        return Ok(());
    }

    // Check certificate expiry
    match check_certificate_expiry(&cert_path) {
        Ok(days) if days < 0 => {
            error!("Certificate expired {} days ago!", -days);
            return Err(anyhow!(
                "Certificate expired. Run 'harness daemon regenerate-certs' to create new certificates"
            ));
        }
        Ok(days) if days < 30 => {
            warn!("Certificate expires in {} days", days);
            if days < 7 {
                warn!("Consider regenerating soon with 'harness daemon regenerate-certs'");
            }
        }
        Ok(days) => {
            info!("Certificate valid for {} more days", days);
        }
        Err(e) => {
            error!("Failed to check certificate expiry: {}", e);
            return Err(e);
        }
    }

    Ok(())
}

/// Regenerate certificates (called from CLI)
pub fn regenerate_certificates(data_dir: &Path) -> Result<()> {
    let cert_dir = data_dir.join("certs");
    let cert_path = cert_dir.join("server.crt");
    let key_path = cert_dir.join("server.key");

    // Backup existing certificates if they exist
    if cert_path.exists() {
        let backup_path = cert_path.with_extension("crt.backup");
        fs::rename(&cert_path, backup_path).context("Failed to backup certificate")?;
        info!("Backed up existing certificate");
    }

    if key_path.exists() {
        let backup_path = key_path.with_extension("key.backup");
        fs::rename(&key_path, backup_path).context("Failed to backup key")?;
        info!("Backed up existing key");
    }

    generate_certificates(&cert_dir)
}

/// Generate new self-signed certificates
fn generate_certificates(cert_dir: &Path) -> Result<()> {
    use rcgen::{CertificateParams, DistinguishedName};

    // Create certificate directory
    fs::create_dir_all(cert_dir).context("Failed to create certificate directory")?;

    // Configure certificate parameters
    let mut params = CertificateParams::default();
    params.not_before = time::OffsetDateTime::now_utc();
    params.not_after = time::OffsetDateTime::now_utc() + time::Duration::days(365);

    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(rcgen::DnType::CommonName, "localhost");
    distinguished_name.push(rcgen::DnType::OrganizationName, "Harness Executor Daemon");
    params.distinguished_name = distinguished_name;

    params.subject_alt_names = vec![
        rcgen::SanType::DnsName("localhost".to_string()),
        rcgen::SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
    ];

    // Generate certificate
    let cert = rcgen::Certificate::from_params(params).context("Failed to generate certificate")?;

    let cert_pem = cert
        .serialize_pem()
        .context("Failed to serialize certificate")?;
    let key_pem = cert.serialize_private_key_pem();

    // Write certificate and key
    let cert_path = cert_dir.join("server.crt");
    let key_path = cert_dir.join("server.key");

    fs::write(&cert_path, cert_pem).context("Failed to write certificate")?;
    fs::write(&key_path, key_pem).context("Failed to write private key")?;

    // Set restrictive permissions on private key (Unix only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&key_path)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&key_path, perms)?;
    }

    // Write README
    let readme = format!(
        r#"# Harness TLS Certificates

These certificates secure communication between the harness CLI and executor daemon.

## Current Certificate
- Generated: {}
- Expires: {}
- Valid for: localhost, 127.0.0.1

## Certificate Details
- Algorithm: RSA 2048
- Self-signed: Yes
- Purpose: Development/Testing

## To Regenerate
Run: `harness daemon regenerate-certs`

## Using Custom Certificates
Replace server.crt and server.key with your own files.
The daemon will use whatever valid certificates are present.

## Security Note
These are self-signed certificates suitable for local development.
For production use, consider using certificates from a trusted CA.
"#,
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
        (Utc::now() + chrono::Duration::days(365)).format("%Y-%m-%d %H:%M:%S UTC")
    );

    fs::write(cert_dir.join("README.md"), readme).context("Failed to write README")?;

    info!("Generated new self-signed certificate valid for 365 days");
    info!("Certificate location: {:?}", cert_path);

    Ok(())
}

/// Check how many days until certificate expires
fn check_certificate_expiry(cert_path: &Path) -> Result<i64> {
    use x509_parser::prelude::*;

    let cert_pem = fs::read_to_string(cert_path).context("Failed to read certificate")?;

    // Find the certificate in the PEM file
    let start = cert_pem
        .find("-----BEGIN CERTIFICATE-----")
        .ok_or_else(|| anyhow!("No certificate found in PEM"))?;
    let end = cert_pem
        .find("-----END CERTIFICATE-----")
        .ok_or_else(|| anyhow!("No certificate end found in PEM"))?;

    let cert_section = &cert_pem[start..end + 25];

    // Extract base64 content
    let base64_content = cert_section
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect::<String>();

    // Decode base64 to DER
    use base64::Engine;
    let cert_der = base64::engine::general_purpose::STANDARD
        .decode(base64_content)
        .context("Failed to decode certificate base64")?;

    // Parse X.509 certificate
    let (_, cert) = X509Certificate::from_der(&cert_der)
        .map_err(|e| anyhow!("Failed to parse certificate: {:?}", e))?;

    // Get expiry time
    let not_after = cert.validity().not_after;
    let expiry_timestamp = not_after.timestamp();
    let expiry_time = DateTime::from_timestamp(expiry_timestamp, 0)
        .ok_or_else(|| anyhow!("Invalid expiry timestamp"))?;

    // Calculate days until expiry
    let now = Utc::now();
    let duration = expiry_time.signed_duration_since(now);
    let days = duration.num_days();

    Ok(days)
}

/// Get certificate information for display
pub fn get_certificate_info(data_dir: &Path) -> Result<String> {
    let cert_path = data_dir.join("certs/server.crt");

    if !cert_path.exists() {
        return Ok("No certificate found".to_string());
    }

    let days = check_certificate_expiry(&cert_path)?;
    let status = if days < 0 {
        format!("EXPIRED {} days ago", -days)
    } else if days < 30 {
        format!("Valid for {} more days (expires soon!)", days)
    } else {
        format!("Valid for {} more days", days)
    };

    Ok(format!(
        "Certificate status: {}\nLocation: {:?}",
        status, cert_path
    ))
}
