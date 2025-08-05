//! Common test utilities for CLI integration tests

use anyhow::Result;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;

/// Test context that manages daemon lifecycle
pub struct CliTestContext {
    pub daemon_process: Option<std::process::Child>,
    pub daemon_port: u16,
    pub test_dir: TempDir,
    pub harness_binary: PathBuf,
}

impl CliTestContext {
    /// Create a new test context with a running daemon
    pub async fn new() -> Result<Self> {
        Self::with_daemon_args(&[]).await
    }

    /// Create a new test context with custom daemon arguments
    pub async fn with_daemon_args(extra_args: &[&str]) -> Result<Self> {
        // Find an available port
        let daemon_port = find_available_port()?;

        // Create a temporary directory for test data
        let test_dir = TempDir::new()?;

        // Build the harness binary path
        let harness_binary = find_harness_binary()?;

        // Start the daemon
        let mut cmd = Command::new(&harness_binary);
        cmd.args(&[
            "daemon",
            "start",
            "--port",
            &daemon_port.to_string(),
            "--state-dir",
            test_dir.path().to_str().unwrap(),
            "--foreground", // Run in foreground for testing
        ]);

        // Add any extra arguments
        for arg in extra_args {
            cmd.arg(arg);
        }

        let mut daemon_process = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;

        // Wait for daemon to be ready
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Verify daemon is running
        if let Ok(Some(status)) = daemon_process.try_wait() {
            anyhow::bail!("Daemon exited prematurely with status: {:?}", status);
        }

        Ok(Self {
            daemon_process: Some(daemon_process),
            daemon_port,
            test_dir,
            harness_binary,
        })
    }

    /// Run a CLI command and return its output
    pub fn run_cli_command(&self, args: &[&str]) -> Result<CliOutput> {
        let output = Command::new(&self.harness_binary)
            .env("HARNESS_DAEMON_PORT", self.daemon_port.to_string())
            .args(args)
            .output()?;

        Ok(CliOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            success: output.status.success(),
            exit_code: output.status.code(),
        })
    }

    /// Run a CLI command with custom environment variables
    pub fn run_cli_command_with_env(
        &self,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> Result<CliOutput> {
        let mut cmd = Command::new(&self.harness_binary);
        cmd.env("HARNESS_DAEMON_PORT", self.daemon_port.to_string());

        for (key, value) in env {
            cmd.env(key, value);
        }

        let output = cmd.args(args).output()?;

        Ok(CliOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            success: output.status.success(),
            exit_code: output.status.code(),
        })
    }

    /// Create a test service configuration file
    pub fn create_test_config(&self, name: &str) -> Result<PathBuf> {
        let config_path = self.test_dir.path().join(format!("{}.yaml", name));
        let config_content = format!(
            r#"
name: {}
services:
  echo-service:
    binary: echo
    args: ["Hello from {}"]
    env:
      TEST_VAR: "test_value"
"#,
            name, name
        );
        std::fs::write(&config_path, config_content)?;
        Ok(config_path)
    }

    /// Create a custom configuration file
    pub fn create_config(&self, filename: &str, content: &str) -> Result<PathBuf> {
        let config_path = self.test_dir.path().join(filename);
        std::fs::write(&config_path, content)?;
        Ok(config_path)
    }

    /// Get the test directory path
    pub fn test_dir(&self) -> &std::path::Path {
        self.test_dir.path()
    }
}

impl Drop for CliTestContext {
    fn drop(&mut self) {
        // Stop the daemon gracefully
        if let Some(mut daemon) = self.daemon_process.take() {
            // Try graceful shutdown first
            let _ = Command::new(&self.harness_binary)
                .env("HARNESS_DAEMON_PORT", self.daemon_port.to_string())
                .args(&["daemon", "stop"])
                .output();

            // Give it time to shut down gracefully
            std::thread::sleep(Duration::from_millis(500));

            // Force kill if still running
            let _ = daemon.kill();
            let _ = daemon.wait();
        }
    }
}

#[derive(Debug)]
pub struct CliOutput {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
    pub exit_code: Option<i32>,
}

impl CliOutput {
    pub fn assert_success(&self) -> &Self {
        if !self.success {
            panic!(
                "Command failed with exit code {:?}\nSTDOUT:\n{}\nSTDERR:\n{}",
                self.exit_code, self.stdout, self.stderr
            );
        }
        self
    }

    pub fn assert_failure(&self) -> &Self {
        if self.success {
            panic!(
                "Command succeeded but was expected to fail\nSTDOUT:\n{}\nSTDERR:\n{}",
                self.stdout, self.stderr
            );
        }
        self
    }

    pub fn assert_contains(&self, text: &str) -> &Self {
        if !self.stdout.contains(text) && !self.stderr.contains(text) {
            panic!(
                "Output does not contain '{}'\nSTDOUT:\n{}\nSTDERR:\n{}",
                text, self.stdout, self.stderr
            );
        }
        self
    }

    pub fn assert_not_contains(&self, text: &str) -> &Self {
        if self.stdout.contains(text) || self.stderr.contains(text) {
            panic!(
                "Output contains '{}' but should not\nSTDOUT:\n{}\nSTDERR:\n{}",
                text, self.stdout, self.stderr
            );
        }
        self
    }

    pub fn assert_stdout_contains(&self, text: &str) -> &Self {
        if !self.stdout.contains(text) {
            panic!(
                "STDOUT does not contain '{}'\nSTDOUT:\n{}",
                text, self.stdout
            );
        }
        self
    }

    pub fn assert_stderr_contains(&self, text: &str) -> &Self {
        if !self.stderr.contains(text) {
            panic!(
                "STDERR does not contain '{}'\nSTDERR:\n{}",
                text, self.stderr
            );
        }
        self
    }

    pub fn assert_exit_code(&self, expected: i32) -> &Self {
        match self.exit_code {
            Some(code) if code == expected => self,
            Some(code) => panic!(
                "Expected exit code {} but got {}\nSTDOUT:\n{}\nSTDERR:\n{}",
                expected, code, self.stdout, self.stderr
            ),
            None => panic!(
                "Expected exit code {} but process was terminated\nSTDOUT:\n{}\nSTDERR:\n{}",
                expected, self.stdout, self.stderr
            ),
        }
    }
}

/// Find an available port for testing
pub fn find_available_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

/// Find the harness binary in the build directory
fn find_harness_binary() -> Result<PathBuf> {
    // Try to find the binary relative to the test executable
    let current_exe = std::env::current_exe()?;

    // Go up to the deps directory, then to the parent, then look for harness
    let harness_path = current_exe
        .parent() // deps/
        .and_then(|p| p.parent()) // debug/ or release/
        .map(|p| p.join("harness"))
        .ok_or_else(|| anyhow::anyhow!("Could not determine harness binary path"))?;

    if harness_path.exists() {
        Ok(harness_path)
    } else {
        // Try with .exe extension on Windows
        let with_exe = harness_path.with_extension("exe");
        if with_exe.exists() {
            Ok(with_exe)
        } else {
            anyhow::bail!("Harness binary not found at {:?}", harness_path)
        }
    }
}

/// Helper to wait for a condition with timeout
pub async fn wait_for<F>(condition: F, timeout: Duration, check_interval: Duration) -> Result<()>
where
    F: Fn() -> bool,
{
    let start = tokio::time::Instant::now();

    while !condition() {
        if start.elapsed() > timeout {
            anyhow::bail!("Timeout waiting for condition");
        }
        tokio::time::sleep(check_interval).await;
    }

    Ok(())
}
