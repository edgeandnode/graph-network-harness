//! Service configuration types and utilities.
//!
//! This module defines the configuration model for services that matches
//! the ADR-007 specification for heterogeneous service orchestration.

use crate::ports::PortConfig;
use crate::resources::ResourceLimits;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Parameter value that can be a string, number, or boolean
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ParamValue {
    /// String value
    String(String),
    /// Unsigned 32-bit integer
    U32(u32),
    /// Boolean value
    Bool(bool),
}

impl ParamValue {
    /// Convert to string for substitution in templates
    pub fn as_string(&self) -> String {
        match self {
            ParamValue::String(s) => s.clone(),
            ParamValue::U32(n) => n.to_string(),
            ParamValue::Bool(b) => b.to_string(),
        }
    }

    /// Try to parse as u32
    pub fn as_u32(&self) -> Option<u32> {
        match self {
            ParamValue::U32(n) => Some(*n),
            ParamValue::String(s) => s.parse().ok(),
            ParamValue::Bool(_) => None,
        }
    }

    /// Try to parse as u16
    pub fn as_u16(&self) -> Option<u16> {
        self.as_u32().and_then(|n| u16::try_from(n).ok())
    }

    /// Try to parse as u64
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            ParamValue::U32(n) => Some(*n as u64),
            ParamValue::String(s) => s.parse().ok(),
            ParamValue::Bool(_) => None,
        }
    }

    /// Try to get as bool
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ParamValue::Bool(b) => Some(*b),
            ParamValue::String(s) => match s.as_str() {
                "true" | "yes" | "1" => Some(true),
                "false" | "no" | "0" => Some(false),
                _ => None,
            },
            ParamValue::U32(n) => Some(*n != 0),
        }
    }
}

impl fmt::Display for ParamValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_string())
    }
}

impl From<String> for ParamValue {
    fn from(s: String) -> Self {
        ParamValue::String(s)
    }
}

impl From<&str> for ParamValue {
    fn from(s: &str) -> Self {
        ParamValue::String(s.to_string())
    }
}

impl From<u32> for ParamValue {
    fn from(n: u32) -> Self {
        ParamValue::U32(n)
    }
}

impl From<bool> for ParamValue {
    fn from(b: bool) -> Self {
        ParamValue::Bool(b)
    }
}

/// Dependency specification for services and tasks
///
/// Supports two syntaxes in YAML:
/// 1. Explicit (currently used): `- service: postgres` or `- task: deploy`
/// 2. Namespaced (alternative): `- services.postgres` or `- tasks.deploy`
///
/// The explicit syntax deserializes directly to Service/Task variants.
/// The namespaced syntax deserializes to Namespaced and needs resolve().
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged, rename_all = "kebab-case")]
pub enum Dependency {
    /// Namespaced dependency string that needs resolution
    /// Examples: "services.postgres", "tasks.deploy", or bare "postgres"
    /// Use resolve() to convert to Service or Task variant
    Namespaced(String),
    /// Explicit service dependency (from `service: name` in YAML)
    Service {
        /// Name of the service this depends on
        service: String,
    },
    /// Explicit task dependency (from `task: name` in YAML)
    Task {
        /// Name of the task this depends on
        task: String,
    },
}

impl Dependency {
    /// Parse a namespaced dependency string into Service or Task variant
    ///
    /// This is only needed for Namespaced variants. Service and Task variants
    /// are already resolved and will be returned unchanged.
    ///
    /// Our YAML configs use the explicit syntax (service:/task:) so they don't
    /// need resolution, but this method enables the alternative string syntax.
    pub fn resolve(&self) -> Self {
        match self {
            Dependency::Namespaced(s) => {
                if let Some(name) = s.strip_prefix("services.") {
                    Dependency::Service {
                        service: name.to_string(),
                    }
                } else if let Some(name) = s.strip_prefix("tasks.") {
                    Dependency::Task {
                        task: name.to_string(),
                    }
                } else {
                    // Default to service for backward compatibility
                    Dependency::Service { service: s.clone() }
                }
            }
            other => other.clone(),
        }
    }
}

/// Configuration for a service to be managed by the orchestrator
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServiceConfig {
    /// Unique service name
    #[serde(default)]
    pub name: String,
    /// Where and how to run the service
    pub target: ServiceTarget,
    /// Services and tasks this service depends on
    #[serde(default)]
    pub depends_on: Vec<Dependency>,
    /// Optional health check configuration
    pub health_check: Option<HealthCheck>,
}

/// Command execution specification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ProcessCommand {
    /// Modern template-based command with parameters
    Template {
        /// Service parameters for substitution
        #[serde(default)]
        params: HashMap<String, ParamValue>,
        /// Command template with {param} substitution
        /// e.g., "anvil --port {port} --chain-id {chain_id}"
        command_template: String,
    },
    /// Legacy raw command as a single string
    Legacy {
        /// Full command to execute (binary + args)
        command: String,
    },
}

/// Service execution target specification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ServiceTarget {
    /// Local process execution (managed)
    #[serde(rename = "process")]
    Process {
        /// Command specification (either template or legacy)
        #[serde(flatten)]
        command: ProcessCommand,
        /// Environment variables (supports {param} substitution with template)
        #[serde(default)]
        env: HashMap<String, String>,
        /// Named port configuration (e.g., http: auto, ws: 8001)
        #[serde(default, skip_serializing_if = "HashMap::is_empty")]
        ports: PortConfig,
        /// Resource limits (memory, CPU) for systemd-run cgroups
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resources: Option<ResourceLimits>,
        /// Working directory (optional)
        #[serde(skip_serializing_if = "Option::is_none")]
        working_dir: Option<String>,
        /// Validation command for idempotent tasks (optional)
        #[serde(skip_serializing_if = "Option::is_none")]
        validation: Option<String>,
    },
    /// Docker container execution (managed)
    #[serde(rename = "docker")]
    Docker {
        /// Service parameters for substitution
        #[serde(default)]
        params: HashMap<String, ParamValue>,
        /// Container image
        image: String,
        /// Command template override (optional - uses image default if not specified)
        #[serde(skip_serializing_if = "Option::is_none")]
        command_template: Option<String>,
        /// Environment variables (supports {param} substitution)
        #[serde(default)]
        env: HashMap<String, String>,
        /// Port mappings (host ports)
        #[serde(default)]
        ports: Vec<u16>,
        /// Volume mounts
        #[serde(default)]
        volumes: Vec<String>,
    },
    /// Attach to existing Docker container
    #[serde(rename = "docker-attach")]
    DockerAttach {
        /// Container name or ID to attach to
        container: String,
        /// Environment variables
        env: HashMap<String, String>,
    },
    /// Attach to existing local process
    #[serde(rename = "process-attach")]
    ProcessAttach {
        /// Process ID to attach to
        pid: Option<u32>,
        /// Process name to search for
        process_name: Option<String>,
        /// Environment variables
        env: HashMap<String, String>,
    },
    /// Remote execution via SSH
    #[serde(rename = "remote-ssh")]
    Remote {
        /// Remote host address
        host: String,
        /// SSH username
        user: String,
        /// Execution mode
        #[serde(flatten)]
        mode: RemoteMode,
        /// Environment variables
        env: HashMap<String, String>,
    },

    /// Remote attach via SSH
    #[serde(rename = "remote-attach")]
    RemoteAttach {
        /// Remote host address
        host: String,
        /// SSH username
        user: String,
        /// Process ID to attach to
        pid: Option<u32>,
        /// Process name to search for
        process_name: Option<String>,
        /// Environment variables
        #[serde(default)]
        env: HashMap<String, String>,
    },
}

/// Command specification for layered execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommandSpec {
    /// Binary to execute
    pub binary: String,
    /// Command line arguments
    pub args: Vec<String>,
}

/// Remote execution mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum RemoteMode {
    /// Execute a binary on the remote host
    Process {
        /// Binary to execute
        binary: String,
        /// Command line arguments
        args: Vec<String>,
    },
}

impl ProcessCommand {
    /// Get parameters if this is a template command
    pub fn params(&self) -> Option<&HashMap<String, ParamValue>> {
        match self {
            ProcessCommand::Template { params, .. } => Some(params),
            ProcessCommand::Legacy { .. } => None,
        }
    }

    /// Substitute parameters in a template string
    pub fn substitute_params(&self, template: &str) -> String {
        match self {
            ProcessCommand::Template { params, .. } => {
                let mut result = template.to_string();
                // Substitute params
                for (key, value) in params {
                    let placeholder = format!("{{{}}}", key);
                    result = result.replace(&placeholder, &value.as_string());
                }
                result
            }
            ProcessCommand::Legacy { .. } => template.to_string(),
        }
    }

    /// Build the command as a vector of strings
    pub fn build_command(&self) -> Vec<String> {
        match self {
            ProcessCommand::Template {
                command_template, ..
            } => {
                let command = self.substitute_params(command_template);
                // Simple split on whitespace - could be improved with shell_words
                command.split_whitespace().map(String::from).collect()
            }
            ProcessCommand::Legacy { command } => {
                // Simple split on whitespace - could be improved with shell_words
                command.split_whitespace().map(String::from).collect()
            }
        }
    }
}

impl ServiceTarget {
    /// Get a parameter value by key
    pub fn get_param(&self, key: &str) -> Option<&ParamValue> {
        let params = match self {
            ServiceTarget::Process {
                command: ProcessCommand::Template { params, .. },
                ..
            } => params,
            ServiceTarget::Docker { params, .. } => params,
            _ => return None,
        };

        params.get(key)
    }

    /// Get a parameter value by key as a string
    pub fn get_param_str(&self, key: &str) -> Option<String> {
        self.get_param(key).map(|v| v.as_string())
    }

    /// Get a parameter value by key and parse as u16
    pub fn get_param_u16(&self, key: &str) -> Option<u16> {
        self.get_param(key).and_then(|v| v.as_u16())
    }

    /// Get a parameter value by key and parse as u32
    pub fn get_param_u32(&self, key: &str) -> Option<u32> {
        self.get_param(key).and_then(|v| v.as_u32())
    }

    /// Get a parameter value by key and parse as u64
    pub fn get_param_u64(&self, key: &str) -> Option<u64> {
        self.get_param(key).and_then(|v| v.as_u64())
    }

    /// Get a parameter value by key and parse as bool
    pub fn get_param_bool(&self, key: &str) -> Option<bool> {
        self.get_param(key).and_then(|v| v.as_bool())
    }

    /// Get a parameter value and parse it with a default
    pub fn get_param_parsed_or<T: std::str::FromStr>(&self, key: &str, default: T) -> T {
        self.get_param_str(key)
            .and_then(|s| s.parse().ok())
            .unwrap_or(default)
    }

    /// Substitute {param} placeholders in a template string
    pub fn substitute_params(&self, template: &str) -> String {
        match self {
            ServiceTarget::Process { command, .. } => command.substitute_params(template),
            ServiceTarget::Docker { params, .. } => {
                let mut result = template.to_string();
                for (key, value) in params {
                    let placeholder = format!("{{{}}}", key);
                    result = result.replace(&placeholder, &value.as_string());
                }
                result
            }
            _ => template.to_string(),
        }
    }

    /// Build the command from the command specification
    pub fn build_command(&self) -> Option<Vec<String>> {
        match self {
            ServiceTarget::Process { command, .. } => Some(command.build_command()),
            ServiceTarget::Docker {
                command_template: Some(template),
                ..
            } => {
                let command = self.substitute_params(template);
                // Simple split on whitespace - could be improved with shell_words
                Some(command.split_whitespace().map(String::from).collect())
            }
            _ => None,
        }
    }

    /// Build environment variables with parameter substitution
    pub fn build_env(&self) -> HashMap<String, String> {
        let env = self.env();
        env.into_iter()
            .map(|(k, v)| (k, self.substitute_params(&v)))
            .collect()
    }

    /// Get environment variables from the target (legacy - doesn't do substitution)
    pub fn env(&self) -> HashMap<String, String> {
        match self {
            ServiceTarget::Process { env, .. } => env.clone(),
            ServiceTarget::Docker { env, .. } => env.clone(),
            ServiceTarget::DockerAttach { env, .. } => env.clone(),
            ServiceTarget::ProcessAttach { env, .. } => env.clone(),
            ServiceTarget::Remote { env, .. } => env.clone(),
            ServiceTarget::RemoteAttach { env, .. } => env.clone(),
        }
    }

    /// Create a new target with updated environment variables
    pub fn with_env(&self, new_env: HashMap<String, String>) -> Self {
        match self {
            ServiceTarget::Process {
                command,
                ports,
                resources,
                working_dir,
                validation,
                ..
            } => ServiceTarget::Process {
                command: command.clone(),
                env: new_env,
                ports: ports.clone(),
                resources: resources.clone(),
                working_dir: working_dir.clone(),
                validation: validation.clone(),
            },
            ServiceTarget::Docker {
                params,
                image,
                command_template,
                ports,
                volumes,
                ..
            } => ServiceTarget::Docker {
                params: params.clone(),
                image: image.clone(),
                command_template: command_template.clone(),
                env: new_env,
                ports: ports.clone(),
                volumes: volumes.clone(),
            },
            ServiceTarget::DockerAttach { container, .. } => ServiceTarget::DockerAttach {
                container: container.clone(),
                env: new_env,
            },
            ServiceTarget::ProcessAttach {
                pid, process_name, ..
            } => ServiceTarget::ProcessAttach {
                pid: *pid,
                process_name: process_name.clone(),
                env: new_env,
            },
            ServiceTarget::Remote {
                host, user, mode, ..
            } => ServiceTarget::Remote {
                host: host.clone(),
                user: user.clone(),
                mode: mode.clone(),
                env: new_env,
            },
            ServiceTarget::RemoteAttach {
                host,
                user,
                pid,
                process_name,
                ..
            } => ServiceTarget::RemoteAttach {
                host: host.clone(),
                user: user.clone(),
                pid: *pid,
                process_name: process_name.clone(),
                env: new_env,
            },
        }
    }

    /// Inject additional parameters into this target
    ///
    /// Runtime params are merged with existing params (runtime takes precedence).
    /// Use `{param_name}` syntax in templates.
    pub fn inject_params(&mut self, runtime_params: HashMap<String, ParamValue>) {
        match self {
            ServiceTarget::Process { command, .. } => {
                if let ProcessCommand::Template { params, .. } = command {
                    for (k, v) in runtime_params {
                        params.insert(k, v);
                    }
                }
            }
            ServiceTarget::Docker { params, .. } => {
                for (k, v) in runtime_params {
                    params.insert(k, v);
                }
            }
            _ => {}
        }
    }

    /// Get the port configuration for this target.
    ///
    /// Returns the port config if available (Process targets only for now).
    pub fn port_config(&self) -> Option<&PortConfig> {
        match self {
            ServiceTarget::Process { ports, .. } => {
                if ports.is_empty() {
                    None
                } else {
                    Some(ports)
                }
            }
            _ => None,
        }
    }

    /// Apply parameter substitution to process validation/command fields
    ///
    /// For Process targets, substitutes `{param}` in validation and command strings
    pub fn substitute_process_fields(&mut self, params: &HashMap<String, ParamValue>) {
        if let ServiceTarget::Process {
            validation,
            command,
            ..
        } = self
        {
            // Substitute in validation
            if let Some(val) = validation {
                for (key, value) in params {
                    let placeholder = format!("{{{}}}", key);
                    *val = val.replace(&placeholder, &value.as_string());
                }
            }
            // Substitute in command template if it's a Template variant
            if let ProcessCommand::Template {
                command_template, ..
            } = command
            {
                for (key, value) in params {
                    let placeholder = format!("{{{}}}", key);
                    *command_template = command_template.replace(&placeholder, &value.as_string());
                }
            }
        }
    }
}

impl ServiceConfig {
    /// Create a new config with updated environment variables
    pub fn with_env(&self, env: HashMap<String, String>) -> Self {
        ServiceConfig {
            name: self.name.clone(),
            target: self.target.with_env(env),
            depends_on: self.depends_on.clone(),
            health_check: self.health_check.clone(),
        }
    }
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthCheck {
    /// Command to run for health check
    pub command: String,
    /// Arguments for health check command
    pub args: Vec<String>,
    /// Interval between health checks in seconds
    pub interval: u64,
    /// Number of consecutive failures before marking unhealthy
    pub retries: u32,
    /// Timeout for each health check in seconds
    pub timeout: u64,
}

impl Default for HealthCheck {
    fn default() -> Self {
        Self {
            command: "true".to_string(),
            args: vec![],
            interval: 30,
            retries: 3,
            timeout: 10,
        }
    }
}

impl HealthCheck {
    /// Substitute `{param}` placeholders in health check command and args
    pub fn substitute_params(&mut self, params: &HashMap<String, ParamValue>) {
        for (key, value) in params {
            let placeholder = format!("{{{}}}", key);
            let value_str = value.as_string();
            self.command = self.command.replace(&placeholder, &value_str);
            for arg in self.args.iter_mut() {
                *arg = arg.replace(&placeholder, &value_str);
            }
        }
    }
}

/// Current status of a service
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum ServiceStatus {
    /// Service is not running
    #[default]
    Stopped,
    /// Service is starting up
    Starting,
    /// Service is running and healthy
    Running,
    /// Service is running but unhealthy
    Unhealthy,
    /// Service has failed
    Failed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_config_serialization() {
        let config = ServiceConfig {
            name: "test-service".to_string(),
            target: ServiceTarget::Process {
                command: ProcessCommand::Legacy {
                    command: "echo hello".to_string(),
                },
                env: HashMap::from([("FOO".to_string(), "bar".to_string())]),
                ports: HashMap::new(),
                resources: None,
                working_dir: Some("/tmp".to_string()),
                validation: None,
            },
            depends_on: vec![Dependency::Service {
                service: "database".to_string(),
            }],
            health_check: Some(HealthCheck {
                command: "curl".to_string(),
                args: vec!["http://localhost:8080/health".to_string()],
                interval: 30,
                retries: 3,
                timeout: 10,
            }),
        };

        let yaml = serde_yaml::to_string(&config).expect("Failed to serialize");
        let deserialized: ServiceConfig =
            serde_yaml::from_str(&yaml).expect("Failed to deserialize");
        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_service_target_with_env() {
        let mut env = HashMap::new();
        env.insert("KEY1".to_string(), "value1".to_string());

        let target = ServiceTarget::Process {
            command: ProcessCommand::Legacy {
                command: "test".to_string(),
            },
            env: HashMap::new(),
            ports: HashMap::new(),
            resources: None,
            working_dir: None,
            validation: None,
        };

        let updated = target.with_env(env.clone());
        assert_eq!(updated.env(), env);
    }

    #[test]
    fn test_docker_target_serialization() {
        let target = ServiceTarget::Docker {
            params: HashMap::new(),
            image: "nginx:latest".to_string(),
            command_template: None,
            env: HashMap::from([("ENV_VAR".to_string(), "value".to_string())]),
            ports: vec![80, 443],
            volumes: vec!["/data:/app/data".to_string()],
        };

        let yaml = serde_yaml::to_string(&target).expect("Failed to serialize");
        let deserialized: ServiceTarget =
            serde_yaml::from_str(&yaml).expect("Failed to deserialize");
        assert_eq!(target, deserialized);
    }

    #[test]
    fn test_dependency_parsing() {
        // Test service dependency
        let service_dep = Dependency::Service {
            service: "postgres".to_string(),
        };
        let yaml = serde_yaml::to_string(&service_dep).expect("Failed to serialize");
        assert_eq!(yaml.trim(), "service: postgres");

        let deserialized: Dependency = serde_yaml::from_str(&yaml).expect("Failed to deserialize");
        assert_eq!(service_dep, deserialized);

        // Test task dependency
        let task_dep = Dependency::Task {
            task: "deploy-contracts".to_string(),
        };
        let yaml = serde_yaml::to_string(&task_dep).expect("Failed to serialize");
        assert_eq!(yaml.trim(), "task: deploy-contracts");

        let deserialized: Dependency = serde_yaml::from_str(&yaml).expect("Failed to deserialize");
        assert_eq!(task_dep, deserialized);
    }

    #[test]
    fn test_namespaced_dependency_parsing() {
        // Test namespaced service dependency
        let yaml = "services.postgres";
        let dep: Dependency =
            serde_yaml::from_str(&format!("\"{}\"", yaml)).expect("Failed to parse");
        assert!(matches!(dep, Dependency::Namespaced(_)));

        let resolved = dep.resolve();
        assert_eq!(
            resolved,
            Dependency::Service {
                service: "postgres".to_string()
            }
        );

        // Test namespaced task dependency
        let yaml = "tasks.deploy-contracts";
        let dep: Dependency =
            serde_yaml::from_str(&format!("\"{}\"", yaml)).expect("Failed to parse");
        assert!(matches!(dep, Dependency::Namespaced(_)));

        let resolved = dep.resolve();
        assert_eq!(
            resolved,
            Dependency::Task {
                task: "deploy-contracts".to_string()
            }
        );

        // Test plain string defaults to service
        let yaml = "some-service";
        let dep: Dependency =
            serde_yaml::from_str(&format!("\"{}\"", yaml)).expect("Failed to parse");
        assert!(matches!(dep, Dependency::Namespaced(_)));

        let resolved = dep.resolve();
        assert_eq!(
            resolved,
            Dependency::Service {
                service: "some-service".to_string()
            }
        );
    }

    #[test]
    fn test_dependencies_in_yaml() {
        let yaml = r#"
depends_on:
  - service: postgres
  - service: redis
  - task: deploy-contracts
  - task: migrate-database
"#;

        #[derive(Deserialize)]
        struct TestConfig {
            depends_on: Vec<Dependency>,
        }

        let config: TestConfig = serde_yaml::from_str(yaml).expect("Failed to parse YAML");
        assert_eq!(config.depends_on.len(), 4);

        match &config.depends_on[0] {
            Dependency::Service { service } => assert_eq!(service, "postgres"),
            _ => panic!("Expected service dependency"),
        }

        match &config.depends_on[2] {
            Dependency::Task { task } => assert_eq!(task, "deploy-contracts"),
            _ => panic!("Expected task dependency"),
        }
    }

    #[test]
    fn test_namespaced_dependencies_in_yaml() {
        let yaml = r#"
depends_on:
  - services.postgres
  - tasks.deploy-contracts
  - services.redis
  - my-other-service
"#;

        #[derive(Deserialize)]
        struct TestConfig {
            depends_on: Vec<Dependency>,
        }

        let config: TestConfig = serde_yaml::from_str(yaml).expect("Failed to parse YAML");
        assert_eq!(config.depends_on.len(), 4);

        // First dependency should resolve to service
        let resolved = config.depends_on[0].resolve();
        assert_eq!(
            resolved,
            Dependency::Service {
                service: "postgres".to_string()
            }
        );

        // Second dependency should resolve to task
        let resolved = config.depends_on[1].resolve();
        assert_eq!(
            resolved,
            Dependency::Task {
                task: "deploy-contracts".to_string()
            }
        );

        // Third dependency should resolve to service
        let resolved = config.depends_on[2].resolve();
        assert_eq!(
            resolved,
            Dependency::Service {
                service: "redis".to_string()
            }
        );

        // Fourth dependency (plain string) defaults to service
        let resolved = config.depends_on[3].resolve();
        assert_eq!(
            resolved,
            Dependency::Service {
                service: "my-other-service".to_string()
            }
        );
    }

    #[test]
    fn test_both_dependency_syntaxes() {
        use crate::task_config::StackConfig;

        // Test that both explicit and namespaced syntaxes work
        let yaml = r#"
        name: test-stack
        services:
          test-service:
            service_type: test
            target:
              type: process
              command: "test"
            depends_on:
              # Explicit syntax (what we use in YAML)
              - service: postgres
              - task: setup-db
              # Namespaced syntax (alternative)
              - services.redis
              - tasks.migrate-schema
              # Plain string (defaults to service for backward compat)
              - mongodb
        "#;

        let config: StackConfig = serde_yaml::from_str(yaml).unwrap();
        let service = config.services.get("test-service").unwrap();
        let deps = &service.orchestration.depends_on;

        // First dependency - explicit service
        assert!(matches!(&deps[0], Dependency::Service { service } if service == "postgres"));

        // Second dependency - explicit task
        assert!(matches!(&deps[1], Dependency::Task { task } if task == "setup-db"));

        // Third dependency - namespaced service (needs resolve)
        assert!(matches!(&deps[2], Dependency::Namespaced(s) if s == "services.redis"));
        assert!(matches!(deps[2].resolve(), Dependency::Service { service } if service == "redis"));

        // Fourth dependency - namespaced task (needs resolve)
        assert!(matches!(&deps[3], Dependency::Namespaced(s) if s == "tasks.migrate-schema"));
        assert!(matches!(deps[3].resolve(), Dependency::Task { task } if task == "migrate-schema"));

        // Fifth dependency - plain string defaults to service
        assert!(matches!(&deps[4], Dependency::Namespaced(s) if s == "mongodb"));
        assert!(
            matches!(deps[4].resolve(), Dependency::Service { service } if service == "mongodb")
        );
    }

    #[test]
    fn test_new_service_target_variants() {
        // Test DockerAttach
        let docker_attach = ServiceTarget::DockerAttach {
            container: "existing-container".to_string(),
            env: HashMap::from([("KEY".to_string(), "value".to_string())]),
        };

        let yaml = serde_yaml::to_string(&docker_attach).expect("Failed to serialize");
        assert!(yaml.contains("type: docker-attach"));
        assert!(yaml.contains("container: existing-container"));

        // Test ProcessAttach
        let process_attach = ServiceTarget::ProcessAttach {
            pid: Some(1234),
            process_name: None,
            env: HashMap::new(),
        };

        let yaml = serde_yaml::to_string(&process_attach).expect("Failed to serialize");
        assert!(yaml.contains("type: process-attach"));
        assert!(yaml.contains("pid: 1234"));

        // Test Remote with process mode
        let remote = ServiceTarget::Remote {
            host: "example.com".to_string(),
            user: "ubuntu".to_string(),
            mode: RemoteMode::Process {
                binary: "myapp".to_string(),
                args: vec!["--port".to_string(), "8080".to_string()],
            },
            env: HashMap::new(),
        };

        let yaml = serde_yaml::to_string(&remote).expect("Failed to serialize");
        assert!(yaml.contains("type: remote-ssh"));
        assert!(yaml.contains("host: example.com"));
        assert!(yaml.contains("binary: myapp"));
    }
}
