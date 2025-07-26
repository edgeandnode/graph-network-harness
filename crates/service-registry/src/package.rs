//! Package management system

use crate::{Error, Result};
use async_fs::{create_dir_all, File};
use futures::io::AsyncReadExt;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A service package
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    /// Package manifest
    pub manifest: PackageManifest,

    /// Path to the package tarball
    pub package_path: PathBuf,

    /// Installation path
    pub install_path: PathBuf,
}

/// Package manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageManifest {
    /// Package name
    pub name: String,

    /// Package version
    pub version: String,

    /// Package description
    pub description: Option<String>,

    /// Service configuration
    pub service: ServiceConfig,

    /// Dependencies
    #[serde(default)]
    pub depends_on: Vec<String>,

    /// System requirements
    #[serde(default)]
    pub requires: SystemRequirements,

    /// Health check configuration
    pub health: Option<HealthConfig>,
}

/// Service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    /// Service type
    #[serde(rename = "type")]
    pub service_type: String,

    /// Ports the service exposes
    #[serde(default)]
    pub ports: Vec<PortConfig>,
}

/// Port configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortConfig {
    /// Port name
    pub name: String,

    /// Port number
    pub port: u16,

    /// Protocol
    pub protocol: String,
}

/// System requirements
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemRequirements {
    /// Required commands
    #[serde(default)]
    pub commands: Vec<String>,

    /// Required libraries
    #[serde(default)]
    pub libraries: Vec<String>,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    /// Health check script path
    pub script: String,

    /// Check interval
    pub interval: String,

    /// Check timeout
    pub timeout: String,
}

/// Package builder
pub struct PackageBuilder {
    name: String,
    version: String,
    source_dir: PathBuf,
    output_dir: PathBuf,
}

impl PackageBuilder {
    /// Create a new package builder
    pub fn new(name: String, version: String, source_dir: PathBuf, output_dir: PathBuf) -> Self {
        Self {
            name,
            version,
            source_dir,
            output_dir,
        }
    }

    /// Build a package
    pub async fn build(&self) -> Result<Package> {
        // Validate package name and version
        let sanitized_name = Self::sanitize_name(&self.name);
        let sanitized_version = Self::sanitize_version(&self.version);

        // Create install path
        let install_path =
            PathBuf::from("/opt").join(format!("{}-{}", sanitized_name, sanitized_version));

        // Load manifest
        let manifest = self.load_manifest().await?;

        // Create package tarball
        let package_filename = format!("{}-{}.tar.gz", sanitized_name, sanitized_version);
        let package_path = self.output_dir.join(&package_filename);

        self.create_tarball(&package_path).await?;

        Ok(Package {
            manifest,
            package_path,
            install_path,
        })
    }

    /// Load manifest from source directory
    async fn load_manifest(&self) -> Result<PackageManifest> {
        let manifest_path = self.source_dir.join("manifest.yaml");

        if !manifest_path.exists() {
            return Err(Error::Package("No manifest.yaml found".to_string()));
        }

        let mut file = File::open(&manifest_path).await?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).await?;

        let manifest: PackageManifest = serde_yaml::from_str(&contents)
            .map_err(|e| Error::Package(format!("Invalid manifest: {}", e)))?;

        // Validate manifest
        if manifest.name != self.name {
            return Err(Error::Package(format!(
                "Manifest name '{}' doesn't match package name '{}'",
                manifest.name, self.name
            )));
        }

        if manifest.version != self.version {
            return Err(Error::Package(format!(
                "Manifest version '{}' doesn't match package version '{}'",
                manifest.version, self.version
            )));
        }

        Ok(manifest)
    }

    /// Create tarball from source directory
    async fn create_tarball(&self, output_path: &Path) -> Result<()> {
        // TODO: Implement tarball creation
        // This would typically use a library like tar-rs or call tar command
        // For now, return an error as placeholder

        Err(Error::Package(
            "Tarball creation not yet implemented".to_string(),
        ))
    }

    /// Sanitize a package name
    pub fn sanitize_name(name: &str) -> String {
        name.chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    /// Sanitize a version string
    pub fn sanitize_version(version: &str) -> String {
        version
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '.' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }
}

/// Package installer
pub struct PackageInstaller;

impl PackageInstaller {
    /// Install a package to the target system
    pub async fn install(package: &Package, target_dir: &Path) -> Result<()> {
        // Create target directory
        create_dir_all(target_dir).await?;

        // Extract package
        Self::extract_package(&package.package_path, target_dir).await?;

        // Validate required scripts exist
        Self::validate_scripts(target_dir).await?;

        Ok(())
    }

    /// Extract package tarball
    async fn extract_package(package_path: &Path, target_dir: &Path) -> Result<()> {
        // TODO: Implement tarball extraction
        // This would typically use tar-rs or call tar command

        Err(Error::Package(
            "Package extraction not yet implemented".to_string(),
        ))
    }

    /// Validate required scripts exist
    async fn validate_scripts(package_dir: &Path) -> Result<()> {
        let scripts_dir = package_dir.join("scripts");

        // Required scripts
        let required = ["start.sh", "stop.sh"];

        for script in &required {
            let script_path = scripts_dir.join(script);
            if !script_path.exists() {
                return Err(Error::Package(format!(
                    "Required script missing: {}",
                    script
                )));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_sanitization() {
        assert_eq!(PackageBuilder::sanitize_name("api-server"), "api-server");
        assert_eq!(PackageBuilder::sanitize_name("api@server"), "api_server");
        assert_eq!(PackageBuilder::sanitize_name("api.server!"), "api_server_");
        assert_eq!(
            PackageBuilder::sanitize_name("../../../evil"),
            "_________evil"
        );
    }

    #[test]
    fn test_version_sanitization() {
        assert_eq!(PackageBuilder::sanitize_version("1.2.3"), "1.2.3");
        assert_eq!(PackageBuilder::sanitize_version("1.2.3-beta"), "1.2.3-beta");
        assert_eq!(
            PackageBuilder::sanitize_version("1.2.3+build"),
            "1.2.3_build"
        );
        assert_eq!(PackageBuilder::sanitize_version("../1.0"), ".._1.0");
    }

    #[smol_potat::test]
    async fn test_install_path_generation() {
        let builder = PackageBuilder::new(
            "api-server".to_string(),
            "1.2.3".to_string(),
            PathBuf::from("/src"),
            PathBuf::from("/out"),
        );
        // This will fail because we haven't implemented tarball creation yet
        // but we can test the path generation logic
        let name = PackageBuilder::sanitize_name("api-server");
        let version = PackageBuilder::sanitize_version("1.2.3");
        let expected_path = PathBuf::from("/opt").join(format!("{}-{}", name, version));

        assert_eq!(expected_path, PathBuf::from("/opt/api-server-1.2.3"));
    }
}
