use anyhow::{Context, Result};
use harness_config::parser;
use std::path::Path;

pub async fn run(config_path: &Path) -> Result<()> {
    println!("Validating {}...", config_path.display());

    // Try to parse the configuration
    let config = parser::parse_file(config_path).context("Failed to parse configuration")?;

    // Basic validation is done during parsing
    println!("✓ Configuration valid");
    println!("  Version: {}", config.version);

    if let Some(name) = &config.name {
        println!("  Name: {}", name);
    }

    println!("  Networks: {}", config.networks.len());
    println!("  Services: {}", config.services.len());

    // Check for any services with missing environment variables
    // This is a dry-run check, we don't fail on missing vars during validate
    for (name, service) in &config.services {
        if !service.env.is_empty() {
            let mut missing_vars = Vec::new();
            for (_, value) in &service.env {
                if value.contains("${") && !value.contains(":-") {
                    // Extract variable name
                    if let Some(start) = value.find("${") {
                        if let Some(end) = value[start..].find('}') {
                            let var = &value[start + 2..start + end];
                            if std::env::var(var).is_err() {
                                missing_vars.push(var);
                            }
                        }
                    }
                }
            }

            if !missing_vars.is_empty() {
                println!(
                    "  ⚠ Service '{}' references undefined environment variables: {}",
                    name,
                    missing_vars.join(", ")
                );
            }
        }
    }

    Ok(())
}
