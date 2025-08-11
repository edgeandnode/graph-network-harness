//! SSH integration tests for LayeredServiceExecutor with SSH layer
//!
//! These tests require Docker to be running and will create a container
//! with SSH server to test actual SSH connectivity.

#![cfg(all(feature = "ssh-tests", feature = "docker-tests"))]
// We need to allow unsafe for atexit handlers
#![allow(unsafe_code)]

use anyhow::{Context, Result};
use service_orchestration::{
    CommandSpec, LayerConfig, LayeredServiceExecutor, ServiceConfig, ServiceExecutor, ServiceTarget,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tracing::info;

// Global container state
static CONTAINER_NAME: &str = "service-orchestration-ssh-test";
static CONTAINER_GUARD: OnceLock<ContainerCleanupGuard> = OnceLock::new();
static INIT_MUTEX: Mutex<()> = Mutex::new(());
static SIGNAL_HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);
static PANIC_HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);
static ATEXIT_HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);

struct ContainerCleanupGuard {
    container_name: String,
}

impl ContainerCleanupGuard {
    fn cleanup(&self) {
        eprintln!("Cleaning up test container: {}", self.container_name);
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
        return;
    }

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
        return;
    }

    let original_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |panic_info| {
        original_hook(panic_info);

        eprintln!("Panic detected, cleaning up test container...");
        if let Some(guard) = CONTAINER_GUARD.get() {
            guard.cleanup();
        }
    }));
}

/// Install atexit handler for cleanup on normal exit
fn install_atexit_handler() {
    if ATEXIT_HANDLER_INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }

    extern "C" fn cleanup_on_exit() {
        eprintln!("Process exiting, cleaning up test container...");
        if let Some(guard) = CONTAINER_GUARD.get() {
            guard.cleanup();
        }
    }

    unsafe {
        libc::atexit(cleanup_on_exit);
    }
}

/// Ensure the SSH container is running for tests
async fn ensure_container_running() -> Result<()> {
    use command_executor::{Command, Executor, Target, backends::LocalLauncher};

    {
        let _lock = INIT_MUTEX.lock().unwrap();

        // Install cleanup handlers
        install_signal_handlers();
        install_panic_handler();
        install_atexit_handler();
    } // Lock is dropped here

    // Check if container is already running
    let executor = Executor::local("container-check");
    let check_cmd = Command::builder("docker")
        .arg("ps")
        .arg("-q")
        .arg("-f")
        .arg(format!("name={}", CONTAINER_NAME))
        .build();

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

    eprintln!("Starting SSH test container...");

    // Find workspace root
    let mut current_dir = std::env::current_dir().context("Failed to get current directory")?;
    let workspace_root = loop {
        let cargo_toml = current_dir.join("Cargo.toml");
        if cargo_toml.exists() {
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
    let ssh_keys_dir = test_dir.join("ssh-keys");

    // Generate SSH keys if needed
    let ssh_key_path = ssh_keys_dir.join("test_ed25519");
    if !ssh_key_path.exists() {
        info!("Generating SSH keys for test");
        std::fs::create_dir_all(&ssh_keys_dir)?;

        let keygen_cmd = Command::builder("ssh-keygen")
            .arg("-t")
            .arg("ed25519")
            .arg("-f")
            .arg(ssh_key_path.to_str().unwrap())
            .arg("-N")
            .arg("")
            .arg("-C")
            .arg("test@service-orchestration")
            .build();

        executor.execute(&Target::Command, keygen_cmd).await?;

        // Copy public key to authorized_keys
        let pub_key_path = ssh_keys_dir.join("test_ed25519.pub");
        let authorized_keys_path = ssh_keys_dir.join("authorized_keys");
        std::fs::copy(&pub_key_path, &authorized_keys_path)?;
    }

    // Build the Docker image
    let build_cmd = Command::builder("docker-compose")
        .arg("-f")
        .arg(test_dir.join("docker-compose.yaml").to_str().unwrap())
        .arg("build")
        .current_dir(&test_dir)
        .build();

    let result = executor.execute(&Target::Command, build_cmd).await?;
    if !result.success() {
        anyhow::bail!("Docker build failed: {}", result.output);
    }

    // Start the container with modified name and port
    let run_cmd = Command::builder("docker")
        .arg("run")
        .arg("-d")
        .arg("--name")
        .arg(CONTAINER_NAME)
        .arg("-p")
        .arg("2224:22")
        .arg("--privileged")
        .arg("-v")
        .arg(format!(
            "{}:/home/testuser/.ssh:ro",
            ssh_keys_dir.to_str().unwrap()
        ))
        .arg("--tmpfs")
        .arg("/run")
        .arg("--tmpfs")
        .arg("/run/lock")
        .arg("--tmpfs")
        .arg("/tmp")
        .arg("-e")
        .arg("container=docker")
        .arg("--stop-signal")
        .arg("SIGRTMIN+3")
        .arg("--security-opt")
        .arg("seccomp:unconfined")
        .arg("command-executor-systemd-ssh-working:latest")
        .build();

    let result = executor.execute(&Target::Command, run_cmd).await?;
    if !result.success() {
        anyhow::bail!("Docker run failed: {}", result.output);
    }

    // Wait for container to be ready
    wait_for_container_ready().await?;

    // Register cleanup guard
    CONTAINER_GUARD.get_or_init(|| ContainerCleanupGuard {
        container_name: CONTAINER_NAME.to_string(),
    });

    eprintln!("SSH test container is ready!");
    Ok(())
}

async fn wait_for_container_ready() -> Result<()> {
    use command_executor::{Command, Executor, Target, backends::LocalLauncher};

    let executor = Executor::local("container-wait");
    let max_attempts = 30;

    // Wait for SSH to be ready
    eprintln!("Waiting for SSH to be ready...");
    for i in 1..=max_attempts {
        let nc_cmd = Command::builder("nc")
            .arg("-z")
            .arg("localhost")
            .arg("2224")
            .build();

        if let Ok(result) = executor.execute(&Target::Command, nc_cmd).await {
            if result.success() {
                eprintln!("SSH is ready on port 2224");
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

/// Get SSH key path for tests
fn get_ssh_key_path() -> Result<PathBuf> {
    let mut current_dir = std::env::current_dir()?;
    let workspace_root = loop {
        let cargo_toml = current_dir.join("Cargo.toml");
        if cargo_toml.exists() {
            let contents = std::fs::read_to_string(&cargo_toml)?;
            if contents.contains("[workspace]") {
                break current_dir;
            }
        }

        if !current_dir.pop() {
            anyhow::bail!("Could not find workspace root");
        }
    };

    Ok(
        workspace_root
            .join("crates/command-executor/tests/systemd-container/ssh-keys/test_ed25519"),
    )
}

/// Read a file from the remote container
async fn read_remote_file(path: &str) -> Result<String> {
    use command_executor::{Command, Executor, Target, backends::LocalLauncher};

    let executor = Executor::local("file-reader");
    let ssh_key_path = get_ssh_key_path()?;

    let ssh_cmd = Command::builder("ssh")
        .arg("-p")
        .arg("2224")
        .arg("-i")
        .arg(ssh_key_path.to_str().unwrap())
        .arg("-o")
        .arg("StrictHostKeyChecking=no")
        .arg("-o")
        .arg("UserKnownHostsFile=/dev/null")
        .arg("testuser@localhost")
        .arg("cat")
        .arg(path)
        .build();

    let result = executor.execute(&Target::Command, ssh_cmd).await?;
    Ok(result.output)
}

/// Test that LayeredServiceExecutor can start a simple process over SSH
#[smol_potat::test]
async fn test_layered_ssh_basic_command() -> Result<()> {
    ensure_container_running().await?;
    let executor = LayeredServiceExecutor::new();
    let ssh_key_path = get_ssh_key_path()?;

    let config = ServiceConfig {
        name: "test-echo".to_string(),
        target: ServiceTarget::Layered {
            layers: vec![
                LayerConfig::Ssh {
                    host: "localhost".to_string(),
                    user: "testuser".to_string(),
                    env: HashMap::new(),
                    port: Some(2224),
                    identity_file: Some(ssh_key_path.to_string_lossy().to_string()),
                    options: vec![
                        "-o".to_string(),
                        "StrictHostKeyChecking=no".to_string(),
                        "-o".to_string(),
                        "UserKnownHostsFile=/dev/null".to_string(),
                    ],
                },
                LayerConfig::Local {
                    env: HashMap::new(),
                    working_dir: None,
                },
            ],
            command: CommandSpec {
                binary: "echo".to_string(),
                args: vec!["Hello from SSH test".to_string()],
            },
        },
        dependencies: vec![],
        health_check: None,
    };

    // Start the service
    let service = executor.start(config).await?;
    assert_eq!(service.name, "test-echo");

    // Give it a moment to complete
    smol::Timer::after(Duration::from_millis(100)).await;

    // Stop the service
    executor.stop(&service).await?;

    Ok(())
}

/// Test that LayeredServiceExecutor can run a long-running process
#[smol_potat::test]
async fn test_layered_ssh_long_running_process() -> Result<()> {
    ensure_container_running().await?;
    let executor = LayeredServiceExecutor::new();
    let ssh_key_path = get_ssh_key_path()?;

    let config = ServiceConfig {
        name: "test-sleep".to_string(),
        target: ServiceTarget::Layered {
            layers: vec![
                LayerConfig::Ssh {
                    host: "localhost".to_string(),
                    user: "testuser".to_string(),
                    env: HashMap::new(),
                    port: Some(2224),
                    identity_file: Some(ssh_key_path.to_string_lossy().to_string()),
                    options: vec![
                        "-o".to_string(),
                        "StrictHostKeyChecking=no".to_string(),
                        "-o".to_string(),
                        "UserKnownHostsFile=/dev/null".to_string(),
                    ],
                },
                LayerConfig::Local {
                    env: HashMap::new(),
                    working_dir: None,
                },
            ],
            command: CommandSpec {
                binary: "sleep".to_string(),
                args: vec!["10".to_string()],
            },
        },
        dependencies: vec![],
        health_check: None,
    };

    // Start the service
    let service = executor.start(config).await?;
    assert!(service.pid.is_some(), "Should have a PID");

    // Verify it's running
    let health = executor.health_check(&service).await?;
    assert!(matches!(
        health,
        service_orchestration::HealthStatus::Healthy
    ));

    // Stop it before it completes
    executor.stop(&service).await?;
    Ok(())
}

/// Test environment variable forwarding over SSH
#[smol_potat::test]
async fn test_layered_ssh_environment_forwarding() -> Result<()> {
    ensure_container_running().await?;
    let executor = LayeredServiceExecutor::new();
    let ssh_key_path = get_ssh_key_path()?;

    let mut env = HashMap::new();
    env.insert("TEST_VARIABLE".to_string(), "Hello from test".to_string());

    let config = ServiceConfig {
        name: "test-env".to_string(),
        target: ServiceTarget::Layered {
            layers: vec![
                LayerConfig::Ssh {
                    host: "localhost".to_string(),
                    user: "testuser".to_string(),
                    env,
                    port: Some(2224),
                    identity_file: Some(ssh_key_path.to_string_lossy().to_string()),
                    options: vec![
                        "-o".to_string(),
                        "StrictHostKeyChecking=no".to_string(),
                        "-o".to_string(),
                        "UserKnownHostsFile=/dev/null".to_string(),
                    ],
                },
                LayerConfig::Local {
                    env: HashMap::new(),
                    working_dir: None,
                },
            ],
            command: CommandSpec {
                binary: "sh".to_string(),
                args: vec![
                    "-c".to_string(),
                    "echo TEST_VARIABLE=$TEST_VARIABLE > /tmp/test-env-output.txt".to_string(),
                ],
            },
        },
        dependencies: vec![],
        health_check: None,
    };

    // Start the service
    let service = executor.start(config).await?;

    // Give it time to write the file
    smol::Timer::after(Duration::from_millis(200)).await;

    // Stop the service
    executor.stop(&service).await?;

    // Verify the environment variable was passed through
    let output = read_remote_file("/tmp/test-env-output.txt").await?;
    assert!(
        output.contains("TEST_VARIABLE=Hello from test"),
        "Environment variable not forwarded correctly"
    );
    Ok(())
}

/// Test multiple concurrent SSH services
#[smol_potat::test]
async fn test_layered_ssh_concurrent_services() -> Result<()> {
    ensure_container_running().await?;
    let executor = LayeredServiceExecutor::new();
    let ssh_key_path = get_ssh_key_path()?;

    let mut services = vec![];

    // Start 3 concurrent services
    for i in 0..3 {
        let config = ServiceConfig {
            name: format!("test-concurrent-{}", i),
            target: ServiceTarget::Layered {
                layers: vec![
                    LayerConfig::Ssh {
                        host: "localhost".to_string(),
                        user: "testuser".to_string(),
                        env: HashMap::new(),
                        port: Some(2224),
                        identity_file: Some(ssh_key_path.to_string_lossy().to_string()),
                        options: vec![
                            "-o".to_string(),
                            "StrictHostKeyChecking=no".to_string(),
                            "-o".to_string(),
                            "UserKnownHostsFile=/dev/null".to_string(),
                        ],
                    },
                    LayerConfig::Local {
                        env: HashMap::new(),
                        working_dir: None,
                    },
                ],
                command: CommandSpec {
                    binary: "sh".to_string(),
                    args: vec![
                        "-c".to_string(),
                        format!(
                            "echo 'Service {}' > /tmp/concurrent-{}.txt && sleep 2",
                            i, i
                        ),
                    ],
                },
            },
            dependencies: vec![],
            health_check: None,
        };

        let service = executor.start(config).await?;
        services.push(service);
    }

    // Give them time to write their files
    smol::Timer::after(Duration::from_millis(500)).await;

    // Verify all are running
    for service in &services {
        let health = executor.health_check(service).await?;
        assert!(matches!(
            health,
            service_orchestration::HealthStatus::Healthy
        ));
    }

    // Stop all services
    for service in services {
        executor.stop(&service).await?;
    }

    // Verify all files were created
    for i in 0..3 {
        let output = read_remote_file(&format!("/tmp/concurrent-{}.txt", i)).await?;
        assert_eq!(output.trim(), format!("Service {}", i));
    }
    Ok(())
}

/// Test SSH with custom port
#[smol_potat::test]
async fn test_layered_ssh_custom_port() -> Result<()> {
    ensure_container_running().await?;
    let executor = LayeredServiceExecutor::new();
    let ssh_key_path = get_ssh_key_path()?;

    let config = ServiceConfig {
        name: "test-custom-port".to_string(),
        target: ServiceTarget::Layered {
            layers: vec![
                LayerConfig::Ssh {
                    host: "localhost".to_string(),
                    user: "testuser".to_string(),
                    env: HashMap::new(),
                    port: Some(2224),
                    identity_file: Some(ssh_key_path.to_string_lossy().to_string()),
                    options: vec![
                        "-o".to_string(),
                        "StrictHostKeyChecking=no".to_string(),
                        "-o".to_string(),
                        "UserKnownHostsFile=/dev/null".to_string(),
                    ],
                },
                LayerConfig::Local {
                    env: HashMap::new(),
                    working_dir: None,
                },
            ],
            command: CommandSpec {
                binary: "hostname".to_string(),
                args: vec![],
            },
        },
        dependencies: vec![],
        health_check: None,
    };

    // Should connect successfully with custom port
    let service = executor.start(config).await?;
    assert!(service.pid.is_some());

    executor.stop(&service).await?;
    Ok(())
}

/// Test SSH with key file authentication
#[smol_potat::test]
async fn test_layered_ssh_key_authentication() -> Result<()> {
    ensure_container_running().await?;
    let executor = LayeredServiceExecutor::new();
    let ssh_key_path = get_ssh_key_path()?;

    let config = ServiceConfig {
        name: "test-key-auth".to_string(),
        target: ServiceTarget::Layered {
            layers: vec![
                LayerConfig::Ssh {
                    host: "localhost".to_string(),
                    user: "testuser".to_string(),
                    env: HashMap::new(),
                    port: Some(2224),
                    identity_file: Some(ssh_key_path.to_string_lossy().to_string()),
                    options: vec![
                        "-o".to_string(),
                        "StrictHostKeyChecking=no".to_string(),
                        "-o".to_string(),
                        "UserKnownHostsFile=/dev/null".to_string(),
                    ],
                },
                LayerConfig::Local {
                    env: HashMap::new(),
                    working_dir: None,
                },
            ],
            command: CommandSpec {
                binary: "whoami".to_string(),
                args: vec![],
            },
        },
        dependencies: vec![],
        health_check: None,
    };

    // Should authenticate with key file
    let service = executor.start(config).await?;
    assert!(service.pid.is_some());

    executor.stop(&service).await?;
    Ok(())
}
