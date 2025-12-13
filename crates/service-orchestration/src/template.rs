//! Template file processing for service configuration.
//!
//! Processes template files with port substitution before service startup.
//! Templates are read from `templates/` and written to `run-data/runs/<run-id>/config/<service>/`.

use crate::config::TemplateConfig;
use crate::ports::PortAllocator;
use std::path::{Path, PathBuf};
use std::result::Result;
use thiserror::Error;

/// Errors that can occur during template processing
#[derive(Error, Debug)]
pub enum TemplateError {
    /// Failed to read template file
    #[error("Failed to read template '{0}': {1}")]
    ReadError(PathBuf, std::io::Error),

    /// Failed to write output file
    #[error("Failed to write output '{0}': {1}")]
    WriteError(PathBuf, std::io::Error),

    /// Failed to create output directory
    #[error("Failed to create directory '{0}': {1}")]
    DirError(PathBuf, std::io::Error),

    /// Port substitution failed
    #[error("Port substitution failed: {0}")]
    PortError(#[from] crate::ports::PortError),

    /// Invalid template path
    #[error("Invalid template path: {0}")]
    InvalidPath(String),
}

/// Context for a single run, managing paths and IDs
#[derive(Debug, Clone)]
pub struct RunContext {
    /// Unique run identifier
    pub run_id: String,
    /// Base directory for run data (e.g., "./run-data")
    pub base_dir: PathBuf,
    /// Templates directory (e.g., "./templates")
    pub templates_dir: PathBuf,
}

impl RunContext {
    /// Create a new run context with a generated run ID
    pub fn new(base_dir: impl Into<PathBuf>, templates_dir: impl Into<PathBuf>) -> Self {
        let run_id = Self::generate_run_id();
        Self {
            run_id,
            base_dir: base_dir.into(),
            templates_dir: templates_dir.into(),
        }
    }

    /// Create a run context with a specific run ID
    pub fn with_run_id(
        run_id: impl Into<String>,
        base_dir: impl Into<PathBuf>,
        templates_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            base_dir: base_dir.into(),
            templates_dir: templates_dir.into(),
        }
    }

    /// Generate a unique run ID (timestamp + short random suffix)
    fn generate_run_id() -> String {
        use std::time::SystemTime;
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let random: u16 = rand::random();
        format!("{}-{:04x}", timestamp, random)
    }

    /// Get the run directory path: run-data/runs/<run-id>/
    pub fn run_dir(&self) -> PathBuf {
        self.base_dir.join("runs").join(&self.run_id)
    }

    /// Get the config directory for a service: run-data/runs/<run-id>/config/<service>/
    pub fn config_dir(&self, service_name: &str) -> PathBuf {
        self.run_dir().join("config").join(service_name)
    }

    /// Get the runtime directory: run-data/runs/<run-id>/runtime/
    pub fn runtime_dir(&self) -> PathBuf {
        self.run_dir().join("runtime")
    }

    /// Get the template source path
    pub fn template_path(&self, source: &str) -> PathBuf {
        self.templates_dir.join(source)
    }

    /// Substitute run context placeholders in a string.
    ///
    /// Supports:
    /// - `{run.id}` - the run ID
    /// - `{run.dir}` - the run directory path
    /// - `{run.config_dir}` - the config directory for the current service
    /// - `{run.runtime_dir}` - the runtime directory
    pub fn substitute(&self, template: &str, service_name: &str) -> String {
        let mut result = template.to_string();

        result = result.replace("{run.id}", &self.run_id);
        result = result.replace("{run.dir}", &self.run_dir().to_string_lossy());
        result = result.replace(
            "{run.config_dir}",
            &self.config_dir(service_name).to_string_lossy(),
        );
        result = result.replace("{run.runtime_dir}", &self.runtime_dir().to_string_lossy());

        result
    }
}

/// Template processor that handles file substitution
pub struct TemplateProcessor<'a> {
    /// Port allocator for resolving port references
    port_allocator: &'a PortAllocator,
    /// Run context for paths
    run_context: &'a RunContext,
}

impl<'a> TemplateProcessor<'a> {
    /// Create a new template processor
    pub fn new(port_allocator: &'a PortAllocator, run_context: &'a RunContext) -> Self {
        Self {
            port_allocator,
            run_context,
        }
    }

    /// Process all templates for a service
    ///
    /// Returns the paths of generated config files
    pub fn process_templates(
        &self,
        service_name: &str,
        templates: &[TemplateConfig],
    ) -> Result<Vec<PathBuf>, TemplateError> {
        let mut output_paths = Vec::new();

        for template in templates {
            let output_path = self.process_template(service_name, template)?;
            output_paths.push(output_path);
        }

        Ok(output_paths)
    }

    /// Process a single template file
    ///
    /// Returns the path of the generated config file
    pub fn process_template(
        &self,
        service_name: &str,
        template: &TemplateConfig,
    ) -> Result<PathBuf, TemplateError> {
        // Read template
        let template_path = self.run_context.template_path(&template.source);
        let content = std::fs::read_to_string(&template_path)
            .map_err(|e| TemplateError::ReadError(template_path.clone(), e))?;

        // Substitute port references
        let processed = self
            .port_allocator
            .substitute_ports(&content, Some(service_name))?;

        // Determine output filename
        let output_filename = template.output.clone().unwrap_or_else(|| {
            // Strip .template suffix if present
            let source_name = Path::new(&template.source)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&template.source);

            source_name
                .strip_suffix(".template")
                .unwrap_or(source_name)
                .to_string()
        });

        // Create output directory
        let output_dir = self.run_context.config_dir(service_name);
        std::fs::create_dir_all(&output_dir)
            .map_err(|e| TemplateError::DirError(output_dir.clone(), e))?;

        // Write output
        let output_path = output_dir.join(&output_filename);
        std::fs::write(&output_path, processed)
            .map_err(|e| TemplateError::WriteError(output_path.clone(), e))?;

        tracing::info!(
            "Processed template {} -> {}",
            template.source,
            output_path.display()
        );

        Ok(output_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{PortConfig, PortSpec};
    use tempfile::TempDir;

    #[test]
    fn test_run_context_paths() {
        let ctx = RunContext::with_run_id("test-123", "/data", "/templates");

        assert_eq!(ctx.run_dir(), PathBuf::from("/data/runs/test-123"));
        assert_eq!(
            ctx.config_dir("graph-node"),
            PathBuf::from("/data/runs/test-123/config/graph-node")
        );
        assert_eq!(
            ctx.runtime_dir(),
            PathBuf::from("/data/runs/test-123/runtime")
        );
        assert_eq!(
            ctx.template_path("graph-node/config.toml.template"),
            PathBuf::from("/templates/graph-node/config.toml.template")
        );
    }

    #[test]
    fn test_template_processing() {
        let temp = TempDir::new().unwrap();
        let templates_dir = temp.path().join("templates");
        let run_data_dir = temp.path().join("run-data");

        // Create template
        std::fs::create_dir_all(templates_dir.join("test-service")).unwrap();
        std::fs::write(
            templates_dir.join("test-service/config.toml.template"),
            "db_port = {postgres.port.main}\nrpc_port = {anvil.port.rpc}",
        )
        .unwrap();

        // Set up port allocator
        let mut allocator = PortAllocator::new(50000, 50100);
        let postgres_ports: PortConfig =
            [("main".to_string(), PortSpec::Fixed(5432))].into_iter().collect();
        let anvil_ports: PortConfig =
            [("rpc".to_string(), PortSpec::Fixed(8545))].into_iter().collect();
        allocator.allocate_for_service("postgres", &postgres_ports).unwrap();
        allocator.allocate_for_service("anvil", &anvil_ports).unwrap();

        // Process template
        let ctx = RunContext::with_run_id("test-run", &run_data_dir, &templates_dir);
        let processor = TemplateProcessor::new(&allocator, &ctx);

        let template_config = TemplateConfig {
            source: "test-service/config.toml.template".to_string(),
            output: None,
        };

        let output_path = processor
            .process_template("test-service", &template_config)
            .unwrap();

        // Verify output
        let content = std::fs::read_to_string(&output_path).unwrap();
        assert_eq!(content, "db_port = 5432\nrpc_port = 8545");
        assert!(output_path.ends_with("config/test-service/config.toml"));
    }
}
