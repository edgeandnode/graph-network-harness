use anyhow::{Context, Result};
use bollard::Docker;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// Information about a synced image
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncedImage {
    pub name: String,
    pub size: Option<i64>,
    pub synced: bool,
    pub error: Option<String>,
}

/// Results of image sync operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSyncResult {
    pub images_found: Vec<String>,
    pub images_synced: Vec<SyncedImage>,
    pub total_synced: usize,
    pub total_skipped: usize,
    pub total_failed: usize,
}

/// Handles syncing Docker images from host to DinD container
pub struct ImageSync {
    docker: Docker,
    container_id: String,
    compose_file: PathBuf,
}

impl ImageSync {
    pub fn new(docker: Docker, container_id: String, compose_file: PathBuf) -> Self {
        Self {
            docker,
            container_id,
            compose_file,
        }
    }

    /// Sync all images from docker-compose.yaml
    pub async fn sync_all(&self) -> Result<ImageSyncResult> {
        info!("Syncing Docker images from host to DinD container...");

        let images = self.parse_compose_images().await?;

        if images.is_empty() {
            warn!("No images found in docker-compose.yaml");
            return Ok(ImageSyncResult {
                images_found: vec![],
                images_synced: vec![],
                total_synced: 0,
                total_skipped: 0,
                total_failed: 0,
            });
        }

        info!("Found {} images in docker-compose.yaml", images.len());
        for image in &images {
            debug!("  - {}", image);
        }

        let mut synced = 0;
        let mut skipped = 0;
        let mut failed = 0;
        let mut synced_images = Vec::new();
        let images_found: Vec<String> = images.iter().cloned().collect();

        for image in images {
            match self.sync_image_with_details(&image).await {
                Ok((true, size)) => {
                    synced += 1;
                    synced_images.push(SyncedImage {
                        name: image,
                        size,
                        synced: true,
                        error: None,
                    });
                }
                Ok((false, size)) => {
                    skipped += 1;
                    synced_images.push(SyncedImage {
                        name: image,
                        size,
                        synced: false,
                        error: None,
                    });
                }
                Err(e) => {
                    failed += 1;
                    warn!("Failed to sync image {}: {}", image, e);
                    synced_images.push(SyncedImage {
                        name: image,
                        size: None,
                        synced: false,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        info!(
            "Image sync complete: {} synced, {} skipped, {} failed",
            synced, skipped, failed
        );
        
        Ok(ImageSyncResult {
            images_found,
            images_synced: synced_images,
            total_synced: synced,
            total_skipped: skipped,
            total_failed: failed,
        })
    }

    /// Parse docker-compose.yaml to extract image names
    pub async fn parse_compose_images(&self) -> Result<HashSet<String>> {
        let content = tokio::fs::read_to_string(&self.compose_file)
            .await
            .context("Failed to read docker-compose.yaml")?;

        let mut images = HashSet::new();

        // Simple regex to find image: lines
        // This handles both quoted and unquoted image names
        let image_re = Regex::new(r#"^\s*image:\s*['""]?([^'""\s]+)['""]?"#)
            .context("Failed to compile image regex")?;

        // Regex to find service names
        let service_re = Regex::new(r#"^\s*([a-zA-Z0-9_-]+):\s*$"#)
            .context("Failed to compile service regex")?;
        
        // Regex to find build directives
        let build_re = Regex::new(r#"^\s*build:"#)
            .context("Failed to compile build regex")?;

        let mut current_service: Option<String> = None;
        let mut service_has_build = false;
        let mut service_has_image = false;

        for line in content.lines() {
            // Check for service definition
            if let Some(captures) = service_re.captures(line) {
                // Process previous service if it had build but no image
                if let Some(service) = &current_service {
                    if service_has_build && !service_has_image {
                        // Construct image name using local-network prefix
                        let image_name = format!("local-network-{}", service);
                        images.insert(image_name);
                    }
                }
                
                // Start tracking new service
                current_service = captures.get(1).map(|m| m.as_str().to_string());
                service_has_build = false;
                service_has_image = false;
                continue;
            }

            // Check for explicit image
            if let Some(captures) = image_re.captures(line) {
                if let Some(image) = captures.get(1) {
                    images.insert(image.as_str().to_string());
                    service_has_image = true;
                }
            }

            // Check for build directive
            if build_re.is_match(line) {
                service_has_build = true;
            }
        }

        // Process last service
        if let Some(service) = &current_service {
            if service_has_build && !service_has_image {
                let image_name = format!("local-network-{}", service);
                images.insert(image_name);
            }
        }

        // Also check for the dev override file if it exists
        let override_file = self
            .compose_file
            .parent()
            .unwrap()
            .join("overrides/indexer-agent-dev/indexer-agent-dev.yaml");

        if override_file.exists() {
            if let Ok(override_content) = tokio::fs::read_to_string(&override_file).await {
                for line in override_content.lines() {
                    if let Some(captures) = image_re.captures(line) {
                        if let Some(image) = captures.get(1) {
                            images.insert(image.as_str().to_string());
                        }
                    }
                }
            }
        }

        // Add commonly built images that might not be in the compose files
        images.insert("ghcr.io/edgeandnode/indexer-agent:latest".to_string());
        images.insert("ghcr.io/edgeandnode/indexer-service-ts:latest".to_string());
        images.insert("ghcr.io/edgeandnode/indexer-gateway:latest".to_string());
        images.insert("ghcr.io/edgeandnode/indexer-tap-agent:latest".to_string());

        Ok(images)
    }

    /// Sync a single image from host to DinD
    /// Returns true if synced, false if skipped
    async fn sync_image(&self, image: &str) -> Result<bool> {
        info!("Checking image: {}", image);

        // Check if image exists on host
        match self.docker.inspect_image(image).await {
            Ok(_) => {
                info!("  Found on host, transferring to DinD...");
                self.transfer_image(image).await?;
                Ok(true)
            }
            Err(_) => {
                info!("  Not found on host, will be pulled in DinD when needed");
                Ok(false)
            }
        }
    }

    /// Sync a single image from host to DinD with size details
    /// Returns (synced, size) where synced is true if transferred, false if skipped
    async fn sync_image_with_details(&self, image: &str) -> Result<(bool, Option<i64>)> {
        info!("Checking image: {}", image);

        // Check if image exists on host
        match self.docker.inspect_image(image).await {
            Ok(image_info) => {
                let size = image_info.size;
                info!("  Found on host (size: {} bytes)", size.unwrap_or(0));
                
                // Check if already exists in DinD before transferring
                let check_cmd = format!(
                    "docker exec {} docker image inspect {} > /dev/null 2>&1",
                    self.container_id, image
                );
                
                let check_output = tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg(&check_cmd)
                    .output()
                    .await
                    .context("Failed to check if image exists in DinD")?;
                
                if check_output.status.success() {
                    info!("  Already exists in DinD container, skipping transfer");
                    Ok((false, size)) // Not transferred, but image exists
                } else {
                    info!("  Transferring to DinD...");
                    self.transfer_image(image).await?;
                    Ok((true, size)) // Successfully transferred
                }
            }
            Err(_) => {
                info!("  Not found on host, will be pulled in DinD when needed");
                Ok((false, None))
            }
        }
    }

    /// Transfer an image from host to DinD container
    /// This method assumes the caller has already verified the image should be transferred
    async fn transfer_image(&self, image: &str) -> Result<()> {
        let cmd = format!(
            "docker save {} | docker exec -i {} docker load",
            image, self.container_id
        );

        info!("  Transferring image to dind container: {}", image);

        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .output()
            .await
            .context("Failed to execute image transfer command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to transfer image: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.is_empty() {
            debug!("docker load output: {}", stdout);
        }

        info!("  Image transferred successfully");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;

    #[tokio::test]
    async fn test_parse_compose_images() {
        let temp_dir = TempDir::new().unwrap();
        let compose_path = temp_dir.path().join("docker-compose.yaml");

        // Write a test docker-compose.yaml
        let compose_content = r#"
version: '3'
services:
  web:
    image: nginx:latest
    ports:
      - "80:80"
  
  db:
    image: "postgres:15-alpine"
    environment:
      POSTGRES_PASSWORD: example
  
  app:
    image: 'myapp:v1.0.0'
    build: .
"#;

        fs::write(&compose_path, compose_content).await.unwrap();

        // Create a mock ImageSync (we'll test parsing only)
        let image_sync = ImageSync {
            docker: Docker::connect_with_local_defaults().unwrap(),
            container_id: "test-container".to_string(),
            compose_file: compose_path,
        };

        let images = image_sync.parse_compose_images().await.unwrap();

        // Should find base images plus our added common images
        assert!(images.contains("nginx:latest"));
        assert!(images.contains("postgres:15-alpine"));
        assert!(images.contains("myapp:v1.0.0"));
        assert!(images.contains("ghcr.io/edgeandnode/indexer-agent:latest"));
    }

    #[tokio::test]
    async fn test_parse_compose_with_override() {
        let temp_dir = TempDir::new().unwrap();
        let compose_path = temp_dir.path().join("docker-compose.yaml");
        let override_dir = temp_dir.path().join("overrides/indexer-agent-dev");
        fs::create_dir_all(&override_dir).await.unwrap();
        let override_path = override_dir.join("indexer-agent-dev.yaml");

        // Write base compose file
        fs::write(
            &compose_path,
            "version: '3'\nservices:\n  web:\n    image: nginx:latest\n",
        )
        .await
        .unwrap();

        // Write override file
        let override_content = r#"
services:
  indexer:
    image: custom-indexer:dev
    ports:
      - "8080:8080"
"#;
        fs::write(&override_path, override_content).await.unwrap();

        let image_sync = ImageSync {
            docker: Docker::connect_with_local_defaults().unwrap(),
            container_id: "test-container".to_string(),
            compose_file: compose_path,
        };

        let images = image_sync.parse_compose_images().await.unwrap();

        assert!(images.contains("nginx:latest"));
        assert!(images.contains("custom-indexer:dev"));
    }

    #[test]
    fn test_image_regex() {
        let re = Regex::new(r#"^\s*image:\s*['""]?([^'""\s]+)['""]?"#).unwrap();

        // Test various image line formats
        let test_cases = vec![
            ("    image: nginx:latest", Some("nginx:latest")),
            (
                "  image: \"postgres:15-alpine\"",
                Some("postgres:15-alpine"),
            ),
            ("    image: 'myapp:v1.0.0'", Some("myapp:v1.0.0")),
            ("image:redis:6", Some("redis:6")),
            ("# image: commented:out", None),
            ("  build: .", None),
        ];

        for (line, expected) in test_cases {
            let captures = re.captures(line);
            match (captures, expected) {
                (Some(cap), Some(exp)) => {
                    assert_eq!(cap.get(1).unwrap().as_str(), exp);
                }
                (None, None) => {
                    // Expected no match
                }
                _ => panic!("Regex match failed for line: {}", line),
            }
        }
    }
}
