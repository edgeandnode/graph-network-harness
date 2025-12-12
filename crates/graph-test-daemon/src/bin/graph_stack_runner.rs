//! Graph Stack Runner
//!
//! Launches the Graph Protocol service stack in a test container for development
//! and integration testing. Services run as sibling processes inside a single
//! Docker container, orchestrated via SSH.
//!
//! Required environment:
//!   GRAPH_NODE_SRC - Path to graph-node repository (for building graph-node binary)
//!
//! Usage:
//!   GRAPH_NODE_SRC=/path/to/graph-node cargo run --bin graph_stack_runner
//!   GRAPH_NODE_SRC=/path/to/graph-node cargo run --bin graph_stack_runner -- --plain

use anyhow::{Context, Result};
use clap::Parser;
use command_executor::{
    backends::LocalLauncher, Command, Launcher, ProcessEventType, ProcessHandle, Target,
};
use console::{style, Emoji};
use futures::StreamExt;
use graph_test_daemon::GraphTestDaemon;
use harness_core::prelude::{BaseDaemon, Daemon};
use indicatif::{ProgressBar, ProgressStyle};
use service_orchestration::StackConfig;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

// Emoji constants
static CHECKMARK: Emoji<'_, '_> = Emoji("✓ ", "+ ");
static CROSS: Emoji<'_, '_> = Emoji("✗ ", "x ");
static WARN: Emoji<'_, '_> = Emoji("⚠ ", "! ");
static ARROW: Emoji<'_, '_> = Emoji("→ ", "-> ");
static BULLET: Emoji<'_, '_> = Emoji("● ", "* ");

#[derive(Parser)]
#[command(name = "graph_stack_runner")]
#[command(about = "Launch the Graph Protocol service stack for testing")]
struct Args {
    /// Use plain text logging instead of fancy output
    #[arg(long)]
    plain: bool,

    /// Config file path (defaults to configs/full-stack-docker-test.yaml)
    #[arg(short, long)]
    config: Option<PathBuf>,
}

/// Output abstraction for fancy vs plain modes
struct Output {
    plain: bool,
}

impl Output {
    fn new(plain: bool) -> Self {
        Self { plain }
    }

    fn header(&self, msg: &str) {
        if self.plain {
            tracing::info!("=== {} ===", msg);
        } else {
            println!();
            println!("{}", style(format!("━━━ {} ━━━", msg)).cyan().bold());
        }
    }

    fn step(&self, msg: &str) {
        if self.plain {
            tracing::info!("{}", msg);
        } else {
            println!("  {} {}", style(ARROW).dim(), msg);
        }
    }

    fn ok(&self, msg: &str) {
        if self.plain {
            tracing::info!("[OK] {}", msg);
        } else {
            println!("  {} {}", style(CHECKMARK).green(), msg);
        }
    }

    fn warn(&self, msg: &str) {
        if self.plain {
            tracing::warn!("{}", msg);
        } else {
            println!("  {} {}", style(WARN).yellow(), style(msg).yellow());
        }
    }

    fn error(&self, msg: &str) {
        if self.plain {
            tracing::error!("{}", msg);
        } else {
            println!("  {} {}", style(CROSS).red(), style(msg).red());
        }
    }

    fn status(&self, name: &str, status: &str, healthy: bool) {
        if self.plain {
            tracing::info!("{}: {}", name, status);
        } else {
            let symbol = if healthy {
                style(BULLET).green()
            } else {
                style(BULLET).red()
            };
            println!("  {} {:16} {}", symbol, name, status);
        }
    }

    fn info(&self, msg: &str) {
        if self.plain {
            tracing::info!("{}", msg);
        } else {
            println!("  {}", style(msg).dim());
        }
    }

    fn spinner(&self, msg: &str) -> Spinner {
        if self.plain {
            tracing::info!("{}", msg);
            Spinner::Plain
        } else {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
                    .template("  {spinner:.cyan} {msg}")
                    .unwrap(),
            );
            pb.set_message(msg.to_string());
            pb.enable_steady_tick(Duration::from_millis(80));
            Spinner::Fancy(pb)
        }
    }
}

enum Spinner {
    Plain,
    Fancy(ProgressBar),
}

impl Spinner {
    fn finish_ok(&self, msg: &str) {
        match self {
            Spinner::Plain => tracing::info!("[OK] {}", msg),
            Spinner::Fancy(pb) => {
                pb.finish_and_clear();
                println!("  {} {}", style(CHECKMARK).green(), msg);
            }
        }
    }

    fn finish_err(&self, msg: &str) {
        match self {
            Spinner::Plain => tracing::error!("[FAIL] {}", msg),
            Spinner::Fancy(pb) => {
                pb.finish_and_clear();
                println!("  {} {}", style(CROSS).red(), style(msg).red());
            }
        }
    }

    fn finish_warn(&self, msg: &str) {
        match self {
            Spinner::Plain => tracing::warn!("{}", msg),
            Spinner::Fancy(pb) => {
                pb.finish_and_clear();
                println!("  {} {}", style(WARN).yellow(), style(msg).yellow());
            }
        }
    }

    fn set_message(&self, msg: &str) {
        if let Spinner::Fancy(pb) = self {
            pb.set_message(msg.to_string());
        }
    }
}

fn validate_graph_node_src() -> Result<PathBuf> {
    match std::env::var("GRAPH_NODE_SRC") {
        Ok(path) => {
            let path = PathBuf::from(path);
            if !path.exists() {
                anyhow::bail!("GRAPH_NODE_SRC path does not exist: {:?}", path);
            }
            if !path.join("Cargo.toml").exists() {
                anyhow::bail!(
                    "GRAPH_NODE_SRC does not appear to be a Cargo project: {:?}",
                    path
                );
            }
            Ok(path)
        }
        Err(_) => {
            eprintln!(
                "{} GRAPH_NODE_SRC environment variable not set\n",
                style("Error:").red().bold()
            );
            eprintln!("Set GRAPH_NODE_SRC to the path of your graph-node repository:");
            eprintln!(
                "  {} GRAPH_NODE_SRC=/path/to/graph-node cargo run --bin graph_stack_runner",
                style("export").cyan()
            );
            std::process::exit(1);
        }
    }
}

#[smol_potat::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Load .env before anything else
    let env_path = dotenvy::dotenv().ok();

    // Initialize output mode
    let out = Output::new(args.plain);

    if args.plain {
        // Plain mode: use tracing
        tracing_subscriber::fmt::init();
    }

    // Print header
    if !args.plain {
        println!();
        println!(
            "{}",
            style("  Graph Stack Runner").cyan().bold()
        );
        println!("{}", style("  ═══════════════════").cyan());
        println!();
    }

    if let Some(path) = env_path {
        out.step(&format!("Loaded env from {}", path.display()));
    }

    // Validate graph-node source
    let graph_node_src = validate_graph_node_src()?;
    out.ok(&format!("GRAPH_NODE_SRC: {}", graph_node_src.display()));

    // Ensure container is running
    out.header("Container");
    ensure_container_running(&out).await?;

    // Load configuration
    out.header("Configuration");
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let manifest_path = PathBuf::from(manifest_dir);

    let config_path = args
        .config
        .unwrap_or_else(|| manifest_path.join("configs/full-stack-docker-test.yaml"));

    std::env::set_current_dir(&manifest_path)?;

    let config = StackConfig::from_file(&config_path).map_err(|e| anyhow::anyhow!(e))?;

    out.ok(&format!("Stack: {}", config.name));
    out.info(&format!(
        "Tasks: {}",
        config.tasks.keys().cloned().collect::<Vec<_>>().join(", ")
    ));
    out.info(&format!(
        "Services: {}",
        config
            .services
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    ));

    // Create daemon
    out.header("Daemon");
    let endpoint: SocketAddr = "127.0.0.1:9445".parse()?;
    let builder = BaseDaemon::builder(config).with_endpoint(endpoint);

    let spinner = out.spinner("Creating daemon...");
    let daemon = GraphTestDaemon::from_builder(builder)
        .await
        .context("Failed to create daemon")?;
    spinner.finish_ok("Daemon created");

    // Start daemon (WebSocket server)
    daemon.start().await.context("Failed to start daemon")?;
    out.ok(&format!("WebSocket server on {}", endpoint));

    // Pre-flight checks
    out.header("Pre-flight");
    let binary_path = graph_node_src.join("target/release/graph-node");
    if binary_path.exists() {
        out.ok("graph-node binary exists (may skip build)");
    } else {
        out.warn("graph-node will be built (this takes a while)");
    }

    // Launch the stack
    out.header("Launching Stack");
    let spinner = out.spinner("Starting services...");

    let event_receiver = daemon
        .launch_stack()
        .await
        .context("Failed to launch stack")?;

    // Process events
    let mut tasks_completed = Vec::new();
    let mut services_started = Vec::new();

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        async_io::Timer::after(Duration::from_millis(100)).await;

        while let Ok(event) = event_receiver.try_recv() {
            match event {
                harness_core::daemon::DaemonEvent::Task(state) => {
                    let name = format!("{:?}", state);
                    if let Some(task_name) = extract_name(&name) {
                        if name.contains("Start") {
                            spinner.set_message(&format!("Task: {}...", task_name));
                        } else if name.contains("Complete") || name.contains("Success") {
                            tasks_completed.push(task_name);
                        }
                    }
                }
                harness_core::daemon::DaemonEvent::Service(state) => {
                    let name = format!("{:?}", state);
                    if let Some(svc_name) = extract_name(&name) {
                        if name.contains("Start") {
                            spinner.set_message(&format!("Service: {}...", svc_name));
                        } else if name.contains("Running") {
                            services_started.push(svc_name);
                        }
                    }
                }
            }
        }

        // Check if we've started expected services
        if services_started.len() >= 4 {
            break;
        }
    }

    spinner.finish_ok(&format!(
        "Launched {} tasks, {} services",
        tasks_completed.len(),
        services_started.len()
    ));

    // Health checks
    out.header("Health Checks");
    let spinner = out.spinner("Checking service health...");
    async_io::Timer::after(Duration::from_secs(3)).await;

    match daemon.base.service_manager().run_health_checks().await {
        Ok(results) if !results.is_empty() => {
            spinner.finish_ok("Health checks complete");
            for (name, status) in &results {
                match status {
                    service_orchestration::HealthStatus::Healthy => {
                        out.status(name, "healthy", true);
                    }
                    service_orchestration::HealthStatus::Unhealthy(msg) => {
                        out.status(name, &format!("unhealthy: {}", msg), false);
                    }
                    service_orchestration::HealthStatus::Unknown => {
                        out.status(name, "unknown", false);
                    }
                }
            }
        }
        Ok(_) => {
            spinner.finish_warn("No health check results from ServiceManager");
            out.step("Checking services via SSH...");
            check_services_manually(&out).await;
        }
        Err(e) => {
            spinner.finish_err(&format!("Health check error: {}", e));
        }
    }

    // Ready
    out.header("Ready");
    if !args.plain {
        println!();
        println!("  Stack is running. Services accessible at:");
        println!(
            "    {} postgres    {}",
            style(BULLET).dim(),
            style("localhost:5432").green()
        );
        println!(
            "    {} ipfs        {}",
            style(BULLET).dim(),
            style("localhost:5001").green()
        );
        println!(
            "    {} anvil       {}",
            style(BULLET).dim(),
            style("localhost:8545").green()
        );
        println!(
            "    {} graph-node  {}",
            style(BULLET).dim(),
            style("localhost:8000").green()
        );
        println!();
        println!(
            "  Press {} to shutdown.",
            style("Ctrl+C").yellow().bold()
        );
        println!();
    } else {
        tracing::info!("Stack is running. Press Ctrl+C to shutdown.");
    }

    // Wait for Ctrl+C
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    ctrlc::set_handler(move || {
        shutdown_clone.store(true, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    while !shutdown.load(Ordering::SeqCst) {
        async_io::Timer::after(Duration::from_millis(100)).await;
    }

    // Shutdown
    out.header("Shutdown");
    let spinner = out.spinner("Stopping daemon...");
    daemon.stop().await.context("Failed to stop daemon")?;
    spinner.finish_ok("Daemon stopped");

    Ok(())
}

fn extract_name(debug_str: &str) -> Option<String> {
    if let Some(start) = debug_str.find("name: \"") {
        let rest = &debug_str[start + 7..];
        if let Some(end) = rest.find('"') {
            return Some(rest[..end].to_string());
        }
    }
    None
}

async fn check_services_manually(out: &Output) {
    use std::process::Command;

    let ssh_key = format!(
        "{}/tests/docker-test-env/ssh-keys/test_ed25519",
        env!("CARGO_MANIFEST_DIR")
    );

    let checks = [
        ("postgres", "pg_isready -h localhost -p 5432"),
        ("ipfs", "ipfs id"),
        ("anvil", "curl -s http://localhost:8545 -X POST -H 'Content-Type: application/json' --data '{\"jsonrpc\":\"2.0\",\"method\":\"eth_blockNumber\",\"params\":[],\"id\":1}'"),
        ("graph-node", "curl -s http://localhost:8000"),
    ];

    for (name, check_cmd) in &checks {
        let result = Command::new("ssh")
            .args([
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "ConnectTimeout=2",
                "-i",
                &ssh_key,
                "-p",
                "2222",
                "testuser@localhost",
                check_cmd,
            ])
            .output();

        let (status, healthy) = match result {
            Ok(output) if output.status.success() => ("running", true),
            Ok(_) => ("not responding", false),
            Err(_) => ("check failed", false),
        };
        out.status(name, status, healthy);
    }
}

async fn ensure_container_running(out: &Output) -> Result<()> {
    use std::process::Command as StdCommand;

    let container_name = "harness-test-runner";

    let status = StdCommand::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", container_name])
        .output()
        .context("Failed to check container status")?;

    if !status.status.success() || !String::from_utf8_lossy(&status.stdout).trim().eq("true") {
        out.step("Container not running, starting...");

        let _ = StdCommand::new("docker")
            .args(["rm", "-f", container_name])
            .output();

        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let docker_test_env = PathBuf::from(manifest_dir).join("tests/docker-test-env");

        // Build container with streaming output
        out.step("Building container image...");

        let mut cmd = Command::new("docker");
        cmd.args(["build", "-t", "harness-test-runner", "."]);
        cmd.current_dir(&docker_test_env);

        let launcher = LocalLauncher;
        let (mut event_stream, mut handle) = launcher
            .launch(&Target::Command, cmd)
            .await
            .context("Failed to start docker build")?;

        // Stream build output
        while let Some(event) = event_stream.next().await {
            match event.event_type {
                ProcessEventType::Stdout | ProcessEventType::Stderr => {
                    if let Some(line) = &event.data {
                        // Show docker build step lines, filter verbose output
                        if line.starts_with("Step ") || line.starts_with("#") || line.contains("ERROR") {
                            out.info(line);
                        }
                    }
                }
                ProcessEventType::Exited { code, .. } => {
                    if code != Some(0) {
                        out.error(&format!("Docker build failed with exit code {:?}", code));
                        anyhow::bail!("Failed to build test container");
                    }
                }
                _ => {}
            }
        }

        // Wait for process to complete
        let exit_result = handle.wait().await.context("Failed to wait for docker build")?;
        if !exit_result.success() {
            out.error("Docker build failed");
            anyhow::bail!("Failed to build test container");
        }
        out.ok("Container image built");

        // Start container
        let spinner = out.spinner("Starting container...");
        let run_status = StdCommand::new("docker")
            .args([
                "run",
                "-d",
                "--name",
                container_name,
                "-p",
                "2222:22",
                "-v",
                "/var/run/docker.sock:/var/run/docker.sock",
                "--privileged",
                "harness-test-runner",
                "/usr/sbin/sshd",
                "-D",
            ])
            .status()
            .context("Failed to start test container")?;

        if !run_status.success() {
            spinner.finish_err("Failed to start container");
            anyhow::bail!("Failed to start test container");
        }
        spinner.finish_ok("Container started");

        let spinner = out.spinner("Waiting for SSH...");
        let ssh_key = format!("{}/tests/docker-test-env/ssh-keys/test_ed25519", manifest_dir);
        for i in 0..30 {
            let ssh_check = StdCommand::new("ssh")
                .args([
                    "-o",
                    "StrictHostKeyChecking=no",
                    "-o",
                    "ConnectTimeout=1",
                    "-i",
                    &ssh_key,
                    "-p",
                    "2222",
                    "testuser@localhost",
                    "echo",
                    "ready",
                ])
                .output();

            if let Ok(output) = ssh_check {
                if output.status.success() {
                    spinner.finish_ok("SSH ready");
                    return Ok(());
                }
            }

            if i % 5 == 4 {
                spinner.set_message(&format!("Waiting for SSH... ({}s)", i + 1));
            }
            async_io::Timer::after(Duration::from_secs(1)).await;
        }
        spinner.finish_err("SSH did not become ready");
        anyhow::bail!("SSH did not become ready in time");
    } else {
        out.ok("Container already running");
    }

    Ok(())
}
