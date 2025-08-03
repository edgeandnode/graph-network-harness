//! Execution layer implementations for common execution contexts.

use super::ExecutionContext;
use crate::{Command, error::Result};

/// Trait for execution layers that can wrap commands
pub trait ExecutionLayer: Send + Sync + std::fmt::Debug {
    /// Wrap a command with this layer's execution context
    fn wrap_command(&self, command: Command, context: &ExecutionContext) -> Result<Command>;
    
    /// Get a description of this layer for debugging
    fn description(&self) -> String;
}

/// Layer for SSH execution - wraps commands to run over SSH
#[derive(Debug, Clone)]
pub struct SshLayer {
    /// SSH destination (user@host or just host)
    pub destination: String,
    /// SSH port (optional)
    pub port: Option<u16>,
    /// SSH identity file (optional)
    pub identity_file: Option<std::path::PathBuf>,
    /// Additional SSH options
    pub options: Vec<String>,
    /// Environment variables to set on the remote host
    pub env: std::collections::HashMap<String, String>,
    /// Working directory on the remote host
    pub working_dir: Option<std::path::PathBuf>,
    /// Enable SSH agent forwarding
    pub agent_forwarding: bool,
    /// Enable X11 forwarding
    pub x11_forwarding: bool,
    /// Allocate a pseudo-TTY
    pub allocate_tty: bool,
}

impl SshLayer {
    /// Create a new SSH layer
    pub fn new(destination: impl Into<String>) -> Self {
        Self {
            destination: destination.into(),
            port: None,
            identity_file: None,
            options: Vec::new(),
            env: std::collections::HashMap::new(),
            working_dir: None,
            agent_forwarding: false,
            x11_forwarding: false,
            allocate_tty: false,
        }
    }
    
    /// Set the SSH port
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }
    
    /// Set the SSH identity file
    pub fn with_identity_file(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.identity_file = Some(path.into());
        self
    }
    
    /// Add an SSH option
    pub fn with_option(mut self, option: impl Into<String>) -> Self {
        self.options.push(option.into());
        self
    }
    
    /// Add an environment variable for the remote host
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }
    
    /// Set the working directory on the remote host
    pub fn with_working_dir(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }
    
    /// Enable SSH agent forwarding (-A flag)
    pub fn with_agent_forwarding(mut self, enabled: bool) -> Self {
        self.agent_forwarding = enabled;
        self
    }
    
    /// Enable X11 forwarding (-X flag)
    pub fn with_x11_forwarding(mut self, enabled: bool) -> Self {
        self.x11_forwarding = enabled;
        self
    }
    
    /// Allocate a pseudo-TTY (-t flag)
    pub fn with_tty(mut self, enabled: bool) -> Self {
        self.allocate_tty = enabled;
        self
    }
}

impl ExecutionLayer for SshLayer {
    fn wrap_command(&self, mut command: Command, _context: &ExecutionContext) -> Result<Command> {
        // Apply SSH layer's own environment variables to the inner command
        for (key, value) in &self.env {
            command.env(key, value);
        }
        
        // Apply SSH layer's working directory to the inner command
        if let Some(workdir) = &self.working_dir {
            command.current_dir(workdir);
        }
        
        let mut ssh_cmd = Command::new("ssh");
        
        // Add forwarding and TTY flags
        if self.agent_forwarding {
            ssh_cmd.arg("-A");
        }
        if self.x11_forwarding {
            ssh_cmd.arg("-X");
        }
        if self.allocate_tty {
            ssh_cmd.arg("-t");
        }
        
        // Add port if specified
        if let Some(port) = self.port {
            ssh_cmd.arg("-p").arg(port.to_string());
        }
        
        // Add identity file if specified
        if let Some(identity) = &self.identity_file {
            ssh_cmd.arg("-i").arg(identity);
        }
        
        // Add custom options
        for option in &self.options {
            ssh_cmd.arg(option);
        }
        
        // Add destination
        ssh_cmd.arg(&self.destination);
        
        // Build remote command with environment variables
        let mut remote_command = String::new();
        
        // Add environment variable assignments for the remote command
        if !self.env.is_empty() {
            let env_assignments: Vec<String> = self.env.iter()
                .map(|(key, value)| format!("{}={}", shell_escape(key.clone()), shell_escape(value.clone())))
                .collect();
            remote_command.push_str(&env_assignments.join(" "));
            remote_command.push(' ');
        }
        
        // Add working directory change if specified
        if let Some(workdir) = &self.working_dir {
            remote_command.push_str(&format!("cd {} && ", shell_escape(workdir.to_string_lossy().to_string())));
        }
        
        // Add the actual command
        remote_command.push_str(&command_to_shell_string(&command)?);
        
        ssh_cmd.arg(remote_command);
        
        Ok(ssh_cmd)
    }
    
    fn description(&self) -> String {
        format!("SSH to {}", self.destination)
    }
}

/// Layer for Docker execution - wraps commands to run in containers
#[derive(Debug, Clone)]
pub struct DockerLayer {
    /// Container name or ID
    pub container: String,
    /// Whether to use interactive mode
    pub interactive: bool,
    /// Whether to allocate a TTY
    pub tty: bool,
    /// User to run as in container
    pub user: Option<String>,
    /// Working directory in container
    pub workdir: Option<String>,
    /// Environment variables to set in the container
    pub env: std::collections::HashMap<String, String>,
}

impl DockerLayer {
    /// Create a new Docker layer
    pub fn new(container: impl Into<String>) -> Self {
        Self {
            container: container.into(),
            interactive: false,
            tty: false,
            user: None,
            workdir: None,
            env: std::collections::HashMap::new(),
        }
    }
    
    /// Enable interactive mode
    pub fn with_interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }
    
    /// Enable TTY allocation
    pub fn with_tty(mut self, tty: bool) -> Self {
        self.tty = tty;
        self
    }
    
    /// Set the user to run as
    pub fn with_user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }
    
    /// Set the working directory
    pub fn with_working_dir(mut self, workdir: impl Into<String>) -> Self {
        self.workdir = Some(workdir.into());
        self
    }
    
    /// Add an environment variable for the container
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }
}

impl ExecutionLayer for DockerLayer {
    fn wrap_command(&self, command: Command, _context: &ExecutionContext) -> Result<Command> {
        let mut docker_cmd = Command::new("docker");
        docker_cmd.arg("exec");
        
        // Add flags
        if self.interactive {
            docker_cmd.arg("-i");
        }
        if self.tty {
            docker_cmd.arg("-t");
        }
        
        // Add user if specified
        if let Some(user) = &self.user {
            docker_cmd.arg("-u").arg(user);
        }
        
        // Add working directory if specified
        if let Some(workdir) = &self.workdir {
            docker_cmd.arg("-w").arg(workdir);
        }
        
        // Add environment variables from this layer as docker -e flags
        for (key, value) in &self.env {
            docker_cmd.arg("-e").arg(format!("{}={}", key, value));
        }
        
        // Add container
        docker_cmd.arg(&self.container);
        
        // Add the command as shell execution
        docker_cmd.arg("sh").arg("-c");
        let command_string = command_to_shell_string(&command)?;
        docker_cmd.arg(command_string);
        
        Ok(docker_cmd)
    }
    
    fn description(&self) -> String {
        format!("Docker exec in {}", self.container)
    }
}

/// Layer for local execution - essentially a pass-through layer
#[derive(Debug, Clone)]
pub struct LocalLayer {
    /// Environment variables to set for local execution
    pub env: std::collections::HashMap<String, String>,
    /// Working directory for local execution
    pub working_dir: Option<std::path::PathBuf>,
}

impl LocalLayer {
    /// Create a new local layer
    pub fn new() -> Self {
        Self {
            env: std::collections::HashMap::new(),
            working_dir: None,
        }
    }
    
    /// Add an environment variable for local execution
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }
    
    /// Set the working directory for local execution
    pub fn with_working_dir(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }
}

impl Default for LocalLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionLayer for LocalLayer {
    fn wrap_command(&self, mut command: Command, _context: &ExecutionContext) -> Result<Command> {
        // Apply environment variables from this layer
        for (key, value) in &self.env {
            command.env(key, value);
        }
        
        // Apply working directory from this layer
        if let Some(workdir) = &self.working_dir {
            command.current_dir(workdir);
        }
        
        Ok(command)
    }
    
    fn description(&self) -> String {
        "Local execution".to_string()
    }
}

/// Convert a Command to a shell-escaped string
fn command_to_shell_string(command: &Command) -> Result<String> {
    let program = command.get_program().to_string_lossy();
    let args: Vec<String> = command.get_args()
        .iter()
        .map(|arg| shell_escape(arg.to_string_lossy().to_string()))
        .collect();
    
    if args.is_empty() {
        Ok(program.to_string())
    } else {
        Ok(format!("{} {}", program, args.join(" ")))
    }
}

/// Escape a string for safe inclusion in a shell command
fn shell_escape(s: String) -> String {
    // Simple escaping for common cases
    // A more robust implementation would handle all shell metacharacters
    if s.contains(|c: char| c.is_whitespace() || "\"'\\$`!*?<>|&;()[]{}".contains(c)) {
        // Use single quotes and escape any single quotes in the string
        format!("'{}'", s.replace('\'', "'\"'\"'"))
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_shell_escape() {
        assert_eq!(shell_escape("simple".to_string()), "simple");
        assert_eq!(shell_escape("with space".to_string()), "'with space'");
        assert_eq!(shell_escape("with'quote".to_string()), "'with'\"'\"'quote'");
        assert_eq!(shell_escape("$variable".to_string()), "'$variable'");
        assert_eq!(shell_escape("path/to/file".to_string()), "path/to/file");
    }
    
    #[test]
    fn test_command_to_shell_string() {
        let mut cmd = Command::new("echo");
        cmd.arg("hello world").arg("$HOME");
        let result = command_to_shell_string(&cmd).unwrap();
        assert_eq!(result, "echo 'hello world' '$HOME'");
    }
    
    #[test]
    fn test_ssh_layer() {
        let layer = SshLayer::new("user@example.com")
            .with_port(2222)
            .with_option("-o StrictHostKeyChecking=no");
        
        let mut cmd = Command::new("ls");
        cmd.arg("-la");
        let context = ExecutionContext::new();
        let result = layer.wrap_command(cmd, &context).unwrap();
        
        let result_string = command_to_shell_string(&result).unwrap();
        assert!(result_string.contains("ssh"));
        assert!(result_string.contains("-p 2222"));
        assert!(result_string.contains("user@example.com"));
        assert!(result_string.contains("'ls -la'"));
    }
    
    #[test]
    fn test_docker_layer() {
        let layer = DockerLayer::new("my-container")
            .with_interactive(true)
            .with_user("root");
        
        let mut cmd = Command::new("ps");
        cmd.arg("aux");
        let context = ExecutionContext::new();
        let result = layer.wrap_command(cmd, &context).unwrap();
        
        let result_string = command_to_shell_string(&result).unwrap();
        assert!(result_string.contains("docker exec"));
        assert!(result_string.contains("-i"));
        assert!(result_string.contains("-u root"));
        assert!(result_string.contains("my-container"));
        assert!(result_string.contains("'ps aux'"));
    }
    
    #[test]
    fn test_local_layer() {
        let layer = LocalLayer::new();
        let context = ExecutionContext::new()
            .with_env("TEST_VAR", "test_value")
            .with_working_dir("/tmp");
        
        let mut cmd = Command::new("echo");
        cmd.arg("test");
        let result = layer.wrap_command(cmd, &context).unwrap();
        
        // Local layer should preserve the command but apply context
        assert_eq!(result.get_program(), "echo");
        assert_eq!(result.get_args().len(), 1);
    }
    
    #[test]
    fn test_layer_descriptions() {
        let ssh_layer = SshLayer::new("user@host");
        let docker_layer = DockerLayer::new("container");
        let local_layer = LocalLayer::new();
        
        assert_eq!(ssh_layer.description(), "SSH to user@host");
        assert_eq!(docker_layer.description(), "Docker exec in container");
        assert_eq!(local_layer.description(), "Local execution");
    }
}