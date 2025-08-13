//! Shared container management for graph-test-daemon tests
//!
//! This module provides a shared Docker container that is started once before all tests
//! and cleaned up after all tests complete, even on signals or panics.

// We use unsafe to register atexit handlers for proper cleanup
#![allow(unsafe_code)]
#![allow(dead_code)]

use anyhow::{Context, Result};
use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

// Store the container name globally so we can clean it up
static CONTAINER_NAME: &str = "graph-test-env";

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
            .args(["stop", &self.container_name])
            .output()
            .ok();
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
                eprintln!("Received signal: {sig:?}");
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

/// Check if Docker is available
pub fn is_docker_available() -> bool {
    std::process::Command::new("docker")
        .arg("version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Setup function that ensures the container is running
/// This can be called by multiple tests safely - it will only start the container once
#[allow(clippy::await_holding_lock)]
pub async fn ensure_container_running() -> Result<()> {
    // Lock to prevent concurrent initialization
    // We need to hold this lock throughout the entire process to prevent race conditions
    let _lock = INIT_MUTEX.lock().unwrap();

    // Check Docker availability first
    if !is_docker_available() {
        anyhow::bail!("Docker is not available");
    }

    // Install signal handlers for cleanup
    install_signal_handlers();

    // Install panic handler for cleanup
    install_panic_handler();

    // Install atexit handler for cleanup on normal exit
    install_atexit_handler();

    // Check if container is already running
    let output = std::process::Command::new("docker")
        .args(["ps", "-q", "-f", &format!("name={}", CONTAINER_NAME)])
        .output()?;

    if !output.stdout.is_empty() {
        eprintln!("Container {} is already running", CONTAINER_NAME);
        return Ok(());
    }

    // Clean up any existing container first
    eprintln!("Cleaning up any existing container...");
    std::process::Command::new("docker")
        .args(["stop", CONTAINER_NAME])
        .output()
        .ok();
    std::process::Command::new("docker")
        .args(["rm", "-f", CONTAINER_NAME])
        .output()
        .ok();

    // Check if the image exists
    let image_name = "graph-test-daemon:test";
    let image_check = std::process::Command::new("docker")
        .args(["images", "-q", image_name])
        .output()?;

    if image_check.stdout.is_empty() {
        eprintln!("Docker image {} not found, building...", image_name);

        // Get the dockerfile directory
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")?;
        let docker_dir = std::path::PathBuf::from(manifest_dir).join("tests/docker-test-env");

        // Build the image
        let build_output = std::process::Command::new("docker")
            .args(["build", "-t", image_name, "."])
            .current_dir(&docker_dir)
            .output()
            .context("Failed to build Docker image")?;

        if !build_output.status.success() {
            let stderr = String::from_utf8_lossy(&build_output.stderr);
            anyhow::bail!("Docker build failed: {}", stderr);
        }
        eprintln!("Docker image built successfully");
    } else {
        eprintln!("Using existing Docker image {}", image_name);
    }

    // Get SSH keys directory
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")?;
    let ssh_keys_dir =
        std::path::PathBuf::from(manifest_dir).join("tests/docker-test-env/ssh-keys");

    // Start the container
    eprintln!("Starting container {}...", CONTAINER_NAME);
    let run_output = std::process::Command::new("docker")
        .args([
            "run",
            "-d",
            "--name",
            CONTAINER_NAME,
            "--privileged",
            "--cgroupns=host",
            "-v",
            "/sys/fs/cgroup:/sys/fs/cgroup:rw",
            "-p",
            "2222:22",
            "-p",
            "5432:5432",
            "-p",
            "5001:5001",
            "-p",
            "8080:8080",
            "-p",
            "8545:8545",
            "-p",
            "8000:8000",
            "-p",
            "8001:8001",
            "-p",
            "8020:8020",
            "-p",
            "8040:8040",
            "-v",
            &format!(
                "{}:/home/testuser/.ssh/authorized_keys:ro",
                ssh_keys_dir.join("authorized_keys").display()
            ),
            image_name,
        ])
        .output()
        .context("Failed to start container")?;

    if !run_output.status.success() {
        let stderr = String::from_utf8_lossy(&run_output.stderr);
        anyhow::bail!("Failed to start container: {}", stderr);
    }

    // Wait for SSH to be ready
    wait_for_ssh_ready().await?;

    // Register cleanup guard
    CONTAINER_GUARD.get_or_init(|| ContainerCleanupGuard {
        container_name: CONTAINER_NAME.to_string(),
    });

    eprintln!("Shared test container is ready!");
    Ok(())
}

async fn wait_for_ssh_ready() -> Result<()> {
    use async_io::Timer;
    use std::time::Duration;

    let max_attempts = 30;

    eprintln!("Waiting for SSH to be ready...");
    for i in 1..=max_attempts {
        // Check if SSH port is open
        let output = std::process::Command::new("nc")
            .args(["-z", "localhost", "2222"])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                eprintln!("SSH is ready on port 2222");

                // Give SSH a moment to fully initialize
                Timer::after(Duration::from_secs(1)).await;
                return Ok(());
            }
        }

        if i == max_attempts {
            anyhow::bail!("Timeout waiting for SSH to be ready");
        }

        eprintln!("Waiting for SSH... ({i}/{max_attempts})");
        Timer::after(Duration::from_secs(1)).await;
    }

    Ok(())
}

/// Manually cleanup the test container
pub async fn cleanup_test_container() {
    eprintln!("Manually cleaning up test container...");
    if let Some(guard) = CONTAINER_GUARD.get() {
        guard.cleanup();
    } else {
        // Even if guard doesn't exist, try to clean up the container
        std::process::Command::new("docker")
            .args(["stop", CONTAINER_NAME])
            .output()
            .ok();
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
                panic!("Container setup failed: {}", e);
            }
        }
    };
}
