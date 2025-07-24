//! Environment variable and service reference resolver using nom parser
//!
//! This module handles resolution of:
//! - Environment variables: ${VAR} and ${VAR:-default}
//! - Service references: ${service.ip}, ${service.port}, ${service.host}

use crate::{Config, ConfigError, Result, Service};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_while1, take_until},
    character::complete::{alpha1, alphanumeric1, char},
    combinator::{map, recognize, verify},
    multi::many0,
    sequence::{delimited, pair, preceded, separated_pair},
    IResult,
};
use std::collections::{HashMap, HashSet};

/// Context for resolving variables and references
#[derive(Debug, Clone)]
pub struct ResolutionContext {
    /// Environment variables (can be overridden)
    pub env_vars: HashMap<String, String>,
    /// Service IP addresses
    pub service_ips: HashMap<String, String>,
    /// Service ports (first exposed port)
    pub service_ports: HashMap<String, u16>,
    /// Service hostnames
    pub service_hosts: HashMap<String, String>,
}

impl ResolutionContext {
    /// Create a new resolution context
    pub fn new() -> Self {
        Self {
            env_vars: std::env::vars().collect(),
            service_ips: HashMap::new(),
            service_ports: HashMap::new(),
            service_hosts: HashMap::new(),
        }
    }

    /// Add or update an environment variable
    pub fn set_env(&mut self, key: String, value: String) {
        self.env_vars.insert(key, value);
    }

    /// Add service network information
    pub fn add_service(&mut self, name: String, ip: String, port: Option<u16>, host: String) {
        self.service_ips.insert(name.clone(), ip);
        self.service_hosts.insert(name.clone(), host);
        if let Some(p) = port {
            self.service_ports.insert(name, p);
        }
    }
}

/// A template variable found in a string
#[derive(Debug, Clone, PartialEq)]
pub enum Variable {
    /// Environment variable with optional default
    EnvVar { name: String, default: Option<String> },
    /// Service reference
    ServiceRef { service: String, property: String },
}

/// Parse an uppercase environment variable name
fn env_var_name(input: &str) -> IResult<&str, &str> {
    verify(
        take_while1(|c: char| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'),
        |s: &str| {
            // Must start with uppercase letter
            s.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false)
        }
    )(input)
}

/// Parse a service name (lowercase identifier)
fn service_name(input: &str) -> IResult<&str, &str> {
    verify(
        recognize(pair(
            alpha1,
            many0(alt((alphanumeric1, tag("_"), tag("-")))),
        )),
        |s: &str| {
            // Service names should be lowercase or have dashes/underscores
            s.chars().next().unwrap().is_alphabetic()
        }
    )(input)
}

/// Parse a service property (ip, port, host)
fn service_property(input: &str) -> IResult<&str, &str> {
    alt((
        tag("ip"),
        tag("port"),
        tag("host"),
    ))(input)
}

/// Parse an environment variable with optional default
fn parse_env_var(input: &str) -> IResult<&str, Variable> {
    alt((
        // With default value
        map(
            separated_pair(env_var_name, tag(":-"), nom::combinator::rest),
            |(name, default)| Variable::EnvVar {
                name: name.to_string(),
                default: Some(default.to_string()),
            },
        ),
        // Without default - consume all remaining input as the variable name
        map(
            nom::combinator::all_consuming(env_var_name),
            |name| Variable::EnvVar {
                name: name.to_string(),
                default: None,
            },
        ),
    ))(input)
}

/// Parse a service reference
fn parse_service_ref(input: &str) -> IResult<&str, Variable> {
    map(
        nom::combinator::all_consuming(separated_pair(service_name, char('.'), service_property)),
        |(service, property)| Variable::ServiceRef {
            service: service.to_string(),
            property: property.to_string(),
        },
    )(input)
}

/// Parse a variable expression (the part inside ${...})
fn parse_variable_expr(input: &str) -> IResult<&str, Result<Variable>> {
    // First check if it contains a dot - if so, it must be a service reference
    if input.contains('.') {
        // If it has a dot, try to parse as service reference
        match parse_service_ref(input) {
            Ok((_, var)) => Ok(("", Ok(var))),
            Err(_) => {
                // Extract service and property for error message
                let parts: Vec<&str> = input.splitn(2, '.').collect();
                if parts.len() == 2 {
                    let service = parts[0];
                    let property = parts[1];
                    
                    // Check what's wrong
                    if service_name(service).is_err() {
                        Ok(("", Err(ConfigError::ValidationError(format!(
                            "Invalid service name '{}' in reference '{}'",
                            service, input
                        )))))
                    } else {
                        Ok(("", Err(ConfigError::ValidationError(format!(
                            "Invalid service reference type '{}' in '{}'",
                            property, input
                        )))))
                    }
                } else {
                    Ok(("", Err(ConfigError::ValidationError(format!(
                        "Invalid service reference format: '{}'",
                        input
                    )))))
                }
            }
        }
    } else {
        // No dot, must be an environment variable
        match parse_env_var(input) {
            Ok((_, var)) => Ok(("", Ok(var))),
            Err(_) => Ok(("", Err(ConfigError::ValidationError(format!(
                "Invalid environment variable name '{}'. Environment variables must be uppercase with underscores",
                input
            ))))),
        }
    }
}

/// Parse a complete variable (${...})
pub fn parse_variable(input: &str) -> IResult<&str, Result<Variable>> {
    let (input, _) = tag("${")(input)?;
    let (input, content) = take_until("}")(input)?;
    let (input, _) = tag("}")(input)?;
    
    match parse_variable_expr(content) {
        Ok((_, result)) => Ok((input, result)),
        Err(_) => Ok((input, Err(ConfigError::ValidationError(format!(
            "Failed to parse variable expression: '{}'", content
        ))))),
    }
}

/// Find all variables in a string
pub fn find_variables(input: &str) -> Vec<Result<(usize, usize, Variable)>> {
    let mut results = Vec::new();
    let mut remaining = input;
    let mut pos = 0;

    while !remaining.is_empty() {
        if let Some(start) = remaining.find("${") {
            pos += start;
            remaining = &remaining[start..];
            
            match parse_variable(remaining) {
                Ok((rest, var_result)) => {
                    let end = pos + (remaining.len() - rest.len());
                    match var_result {
                        Ok(var) => results.push(Ok((pos, end, var))),
                        Err(e) => results.push(Err(e)),
                    }
                    remaining = rest;
                    pos = end;
                }
                Err(_) => {
                    // Skip this ${
                    remaining = &remaining[2..];
                    pos += 2;
                }
            }
        } else {
            break;
        }
    }

    results
}

/// Resolve all variables in a string
pub fn resolve_string(input: &str, context: &ResolutionContext) -> Result<String> {
    let variables = find_variables(input);
    let mut result = String::new();
    let mut last_end = 0;
    let mut errors = Vec::new();
    
    for var_result in variables {
        let (start, end, var) = var_result?;
        
        // Add the part before the variable
        result.push_str(&input[last_end..start]);
        
        // Resolve the variable
        match var {
            Variable::EnvVar { name, default } => {
                if let Some(value) = context.env_vars.get(&name) {
                    result.push_str(value);
                } else if let Ok(value) = std::env::var(&name) {
                    result.push_str(&value);
                } else if let Some(default_value) = default {
                    result.push_str(&default_value);
                } else {
                    errors.push(name);
                }
            }
            Variable::ServiceRef { service, property } => {
                match property.as_str() {
                    "ip" => {
                        if let Some(ip) = context.service_ips.get(&service) {
                            result.push_str(ip);
                        } else {
                            errors.push(format!("{}.{}", service, property));
                        }
                    }
                    "host" => {
                        if let Some(host) = context.service_hosts.get(&service) {
                            result.push_str(host);
                        } else {
                            errors.push(format!("{}.{}", service, property));
                        }
                    }
                    "port" => {
                        if let Some(port) = context.service_ports.get(&service) {
                            result.push_str(&port.to_string());
                        } else {
                            errors.push(format!("{}.{}", service, property));
                        }
                    }
                    _ => {
                        errors.push(format!("{}.{}", service, property));
                    }
                }
            }
        }
        
        last_end = end;
    }
    
    // Add the remaining part
    result.push_str(&input[last_end..]);
    
    if !errors.is_empty() {
        return Err(ConfigError::EnvVarNotFound(errors.join(", ")));
    }
    
    Ok(result)
}

/// Resolve all environment variables in a service configuration
pub fn resolve_service_env(
    service: &Service,
    context: &ResolutionContext,
) -> Result<HashMap<String, String>> {
    let mut resolved_env = HashMap::new();

    for (key, value) in &service.env {
        let resolved_value = resolve_string(value, context)?;
        resolved_env.insert(key.clone(), resolved_value);
    }

    Ok(resolved_env)
}

/// Find all variable references in a configuration
pub fn find_all_references(config: &Config) -> Result<(HashSet<String>, HashSet<String>)> {
    let mut env_vars = HashSet::new();
    let mut service_refs = HashSet::new();

    // Check all service environment variables
    for service in config.services.values() {
        for value in service.env.values() {
            let variables = find_variables(value);
            
            for var_result in variables {
                let (_, _, var) = var_result?;
                match var {
                    Variable::EnvVar { name, .. } => {
                        env_vars.insert(name);
                    }
                    Variable::ServiceRef { service, property } => {
                        service_refs.insert(format!("{}.{}", service, property));
                    }
                }
            }
        }
    }

    Ok((env_vars, service_refs))
}

/// Validate that all references can be resolved
pub fn validate_references(config: &Config) -> Result<()> {
    let (env_vars, service_refs) = find_all_references(config)?;
    
    // Check service references
    for service_ref in &service_refs {
        if let Some(dot_pos) = service_ref.find('.') {
            let service_name = &service_ref[..dot_pos];
            
            if !config.services.contains_key(service_name) {
                return Err(ConfigError::ValidationError(format!(
                    "Service reference '{}' refers to unknown service",
                    service_ref
                )));
            }
        }
    }
    
    // For environment variables, we can only warn since they might be set at runtime
    let missing_env_vars: Vec<String> = env_vars
        .into_iter()
        .filter(|var| std::env::var(var).is_err())
        .collect();
        
    if !missing_env_vars.is_empty() {
        eprintln!(
            "Warning: The following environment variables are not set: {}",
            missing_env_vars.join(", ")
        );
    }
    
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_var_validation() {
        // Test that environment variable parsing enforces uppercase names
        let valid_cases = vec![
            r#"${TEST_VAR}"#,
            r#"${TEST123}"#,
            r#"${TEST_VAR_123}"#,
        ];
        
        for case in valid_cases {
            let result = find_variables(case);
            assert_eq!(result.len(), 1);
            assert!(result[0].is_ok(), "Should parse valid env var: {}", case);
        }
        
        let invalid_cases = vec![
            r#"${test_var}"#,    // lowercase
            r#"${Test_Var}"#,    // mixed case
            r#"${123TEST}"#,     // starts with digit
        ];
        
        for case in invalid_cases {
            let result = find_variables(case);
            assert_eq!(result.len(), 1);
            assert!(result[0].is_err(), "Should reject invalid env var: {}", case);
        }
    }

    #[test]
    fn test_service_name_parser() {
        assert!(service_name("postgres").is_ok());
        assert!(service_name("graph-node").is_ok());
        assert!(service_name("api_server").is_ok());
        assert!(service_name("api123").is_ok());
        assert!(service_name("123api").is_err());
    }

    #[test]
    fn test_parse_variable_expr() {
        // Valid env var
        assert_eq!(
            parse_variable_expr("TEST_VAR").unwrap().1.unwrap(),
            Variable::EnvVar { name: "TEST_VAR".to_string(), default: None }
        );
        
        // Valid env var with default
        assert_eq!(
            parse_variable_expr("TEST_VAR:-default").unwrap().1.unwrap(),
            Variable::EnvVar { name: "TEST_VAR".to_string(), default: Some("default".to_string()) }
        );
        
        // Valid service ref
        assert_eq!(
            parse_variable_expr("postgres.ip").unwrap().1.unwrap(),
            Variable::ServiceRef { service: "postgres".to_string(), property: "ip".to_string() }
        );
        
        // Invalid env var (lowercase)
        assert!(parse_variable_expr("test_var").unwrap().1.is_err());
        
        // Invalid service property
        assert!(parse_variable_expr("postgres.invalid").unwrap().1.is_err());
    }

    #[test]
    fn test_find_variables() {
        let input = "postgresql://${DB_USER}:${DB_PASS:-secret}@${postgres.ip}:${postgres.port}/db";
        let vars = find_variables(input);
        
        assert_eq!(vars.len(), 4);
        
        let parsed_vars: Vec<Variable> = vars.into_iter()
            .map(|r| r.unwrap().2)
            .collect();
            
        assert_eq!(parsed_vars[0], Variable::EnvVar { name: "DB_USER".to_string(), default: None });
        assert_eq!(parsed_vars[1], Variable::EnvVar { name: "DB_PASS".to_string(), default: Some("secret".to_string()) });
        assert_eq!(parsed_vars[2], Variable::ServiceRef { service: "postgres".to_string(), property: "ip".to_string() });
        assert_eq!(parsed_vars[3], Variable::ServiceRef { service: "postgres".to_string(), property: "port".to_string() });
    }

    #[test]
    fn test_invalid_variables() {
        let input = "${invalid_var} ${postgres.invalid}";
        let vars = find_variables(input);
        
        assert_eq!(vars.len(), 2);
        assert!(vars[0].is_err());
        assert!(vars[1].is_err());
    }

    #[test]
    fn test_resolve_string() {
        let mut context = ResolutionContext::new();
        context.set_env("DB_USER".to_string(), "admin".to_string());
        context.add_service(
            "postgres".to_string(),
            "192.168.1.10".to_string(),
            Some(5432),
            "postgres.local".to_string(),
        );

        let result = resolve_string(
            "postgresql://${DB_USER}:${DB_PASS:-secret}@${postgres.ip}:${postgres.port}/db",
            &context,
        )
        .unwrap();

        assert_eq!(result, "postgresql://admin:secret@192.168.1.10:5432/db");
    }
}