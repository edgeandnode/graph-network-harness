//! Shared container management for tests
//!
//! This module provides a shared container that is started once before all tests
//! and cleaned up after all tests complete.

// We use unsafe to register atexit handlers for proper cleanup
#![allow(unsafe_code)]
// These items are used but clippy doesn't detect their usage correctly
#![allow(dead_code)]

use anyhow::{Context, Result};
use command_executor::{Command, Executor, Target};
use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

// Store the container name globally so we can clean it up
static CONTAINER_NAME: &str = "command-executor-systemd-ssh-harness-test";

// Global container guard that will clean up on drop
static CONTAINER_GUARD: OnceLock<ContainerCleanupGuard> = OnceLock::new();

// Flag to track if signal handler is installed
static SIGNAL_HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);

// Flag to track if panic handler is installed
static PANIC_HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);

// Flag to track if atexit handler is installed
static ATEXIT_HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);

// Mutex for container initialization synchronization
static INIT_MUTEX: Mutex<()> = Mutex::new(());

struct ContainerCleanupGuard {
    container_name: String,
}

impl ContainerCleanupGuard {
    fn cleanup(&self) {
        eprintln!("Cleaning up test container: {}", self.container_name);
        // We need to do synchronous cleanup
        std::process::Command::new("docker")
            .args(["rm", "-f", &self.container_name])
            .output()
            .ok();
    }
}

impl Drop for ContainerCleanupGuard {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Install signal handlers for cleanup
fn install_signal_handlers() {
    if SIGNAL_HANDLER_INSTALLED.swap(true, Ordering::SeqCst) {
        // Already installed
        return;
    }

    // Install handlers for common termination signals
    #[cfg(unix)]
    {
        use signal_hook::{
            consts::{SIGINT, SIGTERM},
            iterator::Signals,
        };
        use std::thread;

        let mut signals =
            Signals::new([SIGINT, SIGTERM]).expect("Failed to register signal handler");

        thread::spawn(move || {
            #[allow(clippy::never_loop)]
            for sig in signals.forever() {
                eprintln!("Received signal: {:?}", sig);
                // Cleanup containers before exiting
                if let Some(guard) = CONTAINER_GUARD.get() {
                    guard.cleanup();
                }
                std::process::exit(1);
            }
        });
    }
}

/// Install panic handler for cleanup
fn install_panic_handler() {
    if PANIC_HANDLER_INSTALLED.swap(true, Ordering::SeqCst) {
        // Already installed
        return;
    }

    let original_hook = panic::take_hook();

    panic::set_hook(Box::new(move |panic_info| {
        // Call the original panic handler first
        original_hook(panic_info);

        // Then cleanup our container
        eprintln!("Panic detected, cleaning up test container...");
        if let Some(guard) = CONTAINER_GUARD.get() {
            guard.cleanup();
        }
    }));
}

/// Install atexit handler for cleanup on normal exit
fn install_atexit_handler() {
    if ATEXIT_HANDLER_INSTALLED.swap(true, Ordering::SeqCst) {
        // Already installed
        return;
    }

    extern "C" fn cleanup_on_exit() {
        eprintln!("Process exiting, cleaning up test container...");
        if let Some(guard) = CONTAINER_GUARD.get() {
            guard.cleanup();
        }
    }

    // SAFETY: cleanup_on_exit is a static extern "C" function that doesn't access
    // any invalid memory. The atexit function is a standard C library function
    // that safely registers our cleanup function to be called at process exit.
    // This is necessary because Rust's Drop trait doesn't guarantee execution
    // on process termination (e.g., when killed by signals or panics).
    #[allow(clippy::undocumented_unsafe_blocks)]
    unsafe {
        libc::atexit(cleanup_on_exit);
    }
}

/// Setup function that ensures the container is running
/// This can be called by multiple tests safely - it will only start the container once
#[allow(clippy::await_holding_lock)]
pub async fn ensure_container_running() -> Result<()> {
    // Lock to prevent concurrent initialization
    // We need to hold this lock throughout the entire process to prevent race conditions
    let _lock = INIT_MUTEX.lock().unwrap();

    // Install signal handlers for cleanup
    install_signal_handlers();

    // Install panic handler for cleanup
    install_panic_handler();

    // Install atexit handler for cleanup on normal exit
    install_atexit_handler();

    // Check if container is already running
    let check_cmd = Command::builder("docker")
        .arg("ps")
        .arg("-q")
        .arg("-f")
        .arg(format!("name={}", CONTAINER_NAME))
        .build();

    let executor = Executor::local("container-check");
    let result = executor.execute(&Target::Command, check_cmd).await?;

    if !result.output.trim().is_empty() {
        // Container is already running
        return Ok(());
    }

    // Clean up any stale container first
    eprintln!("Cleaning up any stale test container...");
    let cleanup_cmd = Command::builder("docker")
        .arg("rm")
        .arg("-f")
        .arg(CONTAINER_NAME)
        .build();
    let _ = executor.execute(&Target::Command, cleanup_cmd).await;

    eprintln!("Starting shared test container...");

    // Container not running, start it
    // Find workspace root by looking for Cargo.toml with [workspace]
    let mut current_dir = std::env::current_dir().context("Failed to get current directory")?;
    let workspace_root = loop {
        let cargo_toml = current_dir.join("Cargo.toml");
        if cargo_toml.exists() {
            // Check if this is the workspace root
            let contents = std::fs::read_to_string(&cargo_toml)?;
            if contents.contains("[workspace]") {
                break current_dir;
            }
        }

        if !current_dir.pop() {
            anyhow::bail!("Could not find workspace root");
        }
    };

    let test_dir = workspace_root.join("crates/command-executor/tests/systemd-container");

    // Build the Docker image
    let build_cmd = Command::builder("docker-compose")
        .arg("-f")
        .arg(test_dir.join("docker-compose.yaml").to_str().unwrap())
        .arg("build")
        .current_dir(&test_dir)
        .build();

    let result = executor
        .execute(&Target::Command, build_cmd)
        .await
        .context("Failed to build Docker image")?;

    if !result.success() {
        anyhow::bail!("Docker build failed: {}", result.output);
    }

    // Start the container
    let up_cmd = Command::builder("docker-compose")
        .arg("-f")
        .arg(test_dir.join("docker-compose.yaml").to_str().unwrap())
        .arg("up")
        .arg("-d")
        .current_dir(&test_dir)
        .build();

    let result = executor
        .execute(&Target::Command, up_cmd)
        .await
        .context("Failed to start container")?;

    if !result.success() {
        anyhow::bail!("Docker compose up failed: {}", result.output);
    }

    // Wait for container to be ready
    wait_for_container_ready().await?;

    // Register cleanup guard
    CONTAINER_GUARD.get_or_init(|| ContainerCleanupGuard {
        container_name: CONTAINER_NAME.to_string(),
    });

    eprintln!("Shared test container is ready!");
    Ok(())
}

async fn wait_for_container_ready() -> Result<()> {
    use std::time::Duration;

    let executor = Executor::local("container-wait");
    let max_attempts = 30;

    // Wait for systemd
    eprintln!("Waiting for systemd to initialize...");
    for i in 1..=max_attempts {
        let check_cmd = Command::builder("docker")
            .arg("exec")
            .arg(CONTAINER_NAME)
            .arg("bash")
            .arg("-c")
            .arg("systemctl is-system-running 2>&1 || echo $?")
            .build();

        if let Ok(result) = executor.execute(&Target::Command, check_cmd).await {
            if result.success() || result.output.contains("degraded") {
                eprintln!("Systemd is ready");
                break;
            }
        }

        if i == max_attempts {
            anyhow::bail!("Timeout waiting for systemd");
        }

        eprintln!("Waiting for systemd... ({}/{})", i, max_attempts);
        smol::Timer::after(Duration::from_secs(1)).await;
    }

    // Wait for SSH
    eprintln!("Waiting for SSH to be ready...");
    for i in 1..=max_attempts {
        let nc_cmd = Command::builder("nc")
            .arg("-z")
            .arg("localhost")
            .arg("2223")
            .build();

        if let Ok(result) = executor.execute(&Target::Command, nc_cmd).await {
            if result.success() {
                eprintln!("SSH is ready on port 2223");
                return Ok(());
            }
        }

        if i == max_attempts {
            anyhow::bail!("Timeout waiting for SSH");
        }

        eprintln!("Waiting for SSH... ({}/{})", i, max_attempts);
        smol::Timer::after(Duration::from_secs(1)).await;
    }

    Ok(())
}

/// Get SSH configuration for the shared container
#[cfg(feature = "ssh")]
pub fn get_ssh_config() -> command_executor::backends::ssh::SshConfig {
    // Find workspace root
    let mut current_dir = std::env::current_dir().unwrap();
    let workspace_root = loop {
        let cargo_toml = current_dir.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(contents) = std::fs::read_to_string(&cargo_toml) {
                if contents.contains("[workspace]") {
                    break current_dir;
                }
            }
        }

        if !current_dir.pop() {
            panic!("Could not find workspace root");
        }
    };

    let ssh_key_path = workspace_root
        .join("crates/command-executor/tests/systemd-container/ssh-keys/test_ed25519");

    command_executor::backends::ssh::SshConfig::new("localhost")
        .with_user("testuser")
        .with_port(2223)
        .with_identity_file(&ssh_key_path)
        .with_extra_arg("-o")
        .with_extra_arg("StrictHostKeyChecking=no")
        .with_extra_arg("-o")
        .with_extra_arg("UserKnownHostsFile=/dev/null")
}

/// Manually cleanup the test container
pub async fn cleanup_test_container() {
    eprintln!("Manually cleaning up test container...");
    if let Some(guard) = CONTAINER_GUARD.get() {
        guard.cleanup();
    } else {
        // Even if guard doesn't exist, try to clean up the container
        std::process::Command::new("docker")
            .args(["rm", "-f", CONTAINER_NAME])
            .output()
            .ok();
    }
}

/// Helper macro to setup shared container for a test
#[macro_export]
macro_rules! with_shared_container {
    ($test_body:expr) => {
        match $crate::common::shared_container::ensure_container_running().await {
            Ok(()) => $test_body,
            Err(e) => {
                eprintln!("Failed to ensure container is running: {}", e);
                panic!("Container setup failed");
            }
        }
    };
}
