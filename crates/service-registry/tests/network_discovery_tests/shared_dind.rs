//! Shared Docker-in-Docker container management for network discovery tests
//!
//! This module provides a shared DinD container that is started once before all tests
//! and cleaned up after all tests complete.

use anyhow::{Context, Result};
use command_executor::{Command, Executor, Target, backends::local::LocalLauncher};
use std::panic;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

// Store the container name globally so we can clean it up
pub static DIND_CONTAINER_NAME: &str = "graph-network-dind-test";

// Global container guard that will clean up on drop
static CONTAINER_GUARD: OnceLock<DindContainerGuard> = OnceLock::new();

// Flag to track if signal handler is installed
static SIGNAL_HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);

// Flag to track if panic handler is installed
static PANIC_HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);

// Flag to track if atexit handler is installed
static ATEXIT_HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);

// Mutex for container initialization synchronization
static INIT_MUTEX: Mutex<()> = Mutex::new(());

// Mutex for test execution synchronization (to prevent network conflicts)
pub static TEST_MUTEX: Mutex<()> = Mutex::new(());

struct DindContainerGuard {
    container_name: String,
}

impl DindContainerGuard {
    fn cleanup(&self) {
        eprintln!("Cleaning up DinD test container: {}", self.container_name);
        // We need to do synchronous cleanup
        std::process::Command::new("docker")
            .args(&["rm", "-f", &self.container_name])
            .output()
            .ok();
    }
}

impl Drop for DindContainerGuard {
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
            Signals::new(&[SIGINT, SIGTERM]).expect("Failed to register signal handler");

        thread::spawn(move || {
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
        eprintln!("Panic detected, cleaning up DinD test container...");
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
        eprintln!("Process exiting, cleaning up DinD test container...");
        if let Some(guard) = CONTAINER_GUARD.get() {
            guard.cleanup();
        }
    }

    unsafe {
        libc::atexit(cleanup_on_exit);
    }
}

/// Ensure the shared Docker-in-Docker container is running
/// This can be called by multiple tests safely - it will only start the container once
pub async fn ensure_dind_container_running() -> Result<()> {
    // Lock to prevent concurrent initialization
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
        .arg(format!("name={}", DIND_CONTAINER_NAME))
        .build();

    let launcher = LocalLauncher;
    let executor = Executor::new("dind-check".to_string(), launcher);
    let result = executor.execute(&Target::Command, check_cmd).await?;

    if !result.output.trim().is_empty() {
        // Container is already running
        return Ok(());
    }

    // Remove any existing container with same name
    let cleanup_cmd = Command::builder("docker")
        .arg("rm")
        .arg("-f")
        .arg(DIND_CONTAINER_NAME)
        .build();
    let _ = executor.execute(&Target::Command, cleanup_cmd).await;

    // Start Docker-in-Docker container with privileged mode
    let docker_cmd = Command::builder("docker")
        .arg("run")
        .arg("-d")
        .arg("--name")
        .arg(DIND_CONTAINER_NAME)
        .arg("--privileged")
        .arg("-e")
        .arg("DOCKER_TLS_CERTDIR=")
        .arg("docker:dind")
        .build();

    let result = executor.execute(&Target::Command, docker_cmd).await?;
    if !result.success() {
        anyhow::bail!(
            "Failed to start Docker-in-Docker container: {}",
            result.output
        );
    }

    // Wait for Docker daemon to be ready with retries
    wait_for_docker_daemon_ready(&executor).await?;

    // Copy compose files into container
    copy_compose_files_to_container(&executor).await?;

    // Register cleanup guard
    CONTAINER_GUARD.get_or_init(|| DindContainerGuard {
        container_name: DIND_CONTAINER_NAME.to_string(),
    });

    eprintln!("Shared DinD test container is ready!");
    Ok(())
}

async fn wait_for_docker_daemon_ready(executor: &Executor<LocalLauncher>) -> Result<()> {
    use std::time::Duration;

    let max_attempts = 60;
    eprintln!("Waiting for Docker daemon to be ready in DinD container...");

    for i in 1..=max_attempts {
        let cmd = Command::builder("docker")
            .arg("exec")
            .arg(DIND_CONTAINER_NAME)
            .arg("docker")
            .arg("info")
            .build();

        if let Ok(result) = executor.execute(&Target::Command, cmd).await {
            if result.success() {
                eprintln!("Docker daemon is ready!");
                return Ok(());
            }
        }

        if i == max_attempts {
            anyhow::bail!("Timeout waiting for Docker daemon to be ready");
        }

        eprintln!("Waiting for Docker daemon... ({}/{})", i, max_attempts);
        smol::Timer::after(Duration::from_secs(1)).await;
    }

    Ok(())
}

async fn copy_compose_files_to_container(executor: &Executor<LocalLauncher>) -> Result<()> {
    let compose_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/network_tests/docker-compose");

    let cmd = Command::builder("docker")
        .arg("cp")
        .arg(compose_dir.to_str().unwrap())
        .arg(format!("{}:/compose", DIND_CONTAINER_NAME))
        .build();

    let result = executor.execute(&Target::Command, cmd).await?;

    if !result.success() {
        anyhow::bail!(
            "Failed to copy compose files to container: {}",
            result.output
        );
    }

    Ok(())
}

/// Execute a docker-compose command inside the shared DinD container
pub async fn dind_compose_command(
    compose_file: &str,
    project_name: &str,
    args: Vec<&str>,
) -> Result<()> {
    let launcher = LocalLauncher;
    let executor = Executor::new("dind-compose".to_string(), launcher);

    let mut cmd = Command::builder("docker")
        .arg("exec")
        .arg(DIND_CONTAINER_NAME)
        .arg("docker")
        .arg("compose")
        .arg("-f")
        .arg(format!("/compose/{}", compose_file))
        .arg("-p")
        .arg(project_name);

    for arg in args {
        cmd = cmd.arg(arg);
    }

    let cmd = cmd.build();
    let result = executor.execute(&Target::Command, cmd).await?;

    if !result.success() {
        anyhow::bail!("Docker compose command failed: {}", result.output);
    }

    Ok(())
}

/// Check if Docker is available on the host system
pub async fn check_docker() -> bool {
    let launcher = LocalLauncher;
    let executor = Executor::new("docker-check".to_string(), launcher);

    let cmd = Command::builder("docker").arg("ps").build();

    executor
        .execute(&Target::Command, cmd)
        .await
        .map(|r| r.success())
        .unwrap_or(false)
}

/// Manually cleanup the test container
pub async fn cleanup_dind_container() {
    eprintln!("Manually cleaning up DinD test container...");
    if let Some(guard) = CONTAINER_GUARD.get() {
        guard.cleanup();
    } else {
        // Even if guard doesn't exist, try to clean up the container
        std::process::Command::new("docker")
            .args(&["rm", "-f", DIND_CONTAINER_NAME])
            .output()
            .ok();
    }
}
