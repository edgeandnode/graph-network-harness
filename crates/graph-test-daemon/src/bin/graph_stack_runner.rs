//! Graph Stack Runner
//!
//! Launches the Graph Protocol service stack for development and integration testing.
//! Services run directly on the host as local processes or Docker containers.
//!
//! Usage:
//!   cargo run --bin graph_stack_runner
//!   cargo run --bin graph_stack_runner -- --plain
//!   cargo run --bin graph_stack_runner -- --config path/to/config.yaml

use anyhow::{Context, Result};
use clap::Parser;
use console::{Emoji, style};
use graph_test_daemon::GraphTestDaemon;
use harness_core::prelude::{BaseDaemon, Daemon};
use indicatif::{ProgressBar, ProgressStyle};
use service_orchestration::StackConfig;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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

    /// Config file path (defaults to configs/graph-stack.yaml)
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// WebSocket server port
    #[arg(long, default_value = "9445")]
    port: u16,
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

#[smol_potat::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Load .env before anything else
    let env_path = dotenvy::dotenv().ok();

    // Initialize output mode
    let out = Output::new(args.plain);

    if args.plain {
        tracing_subscriber::fmt::init();
    }

    // Print header
    if !args.plain {
        println!();
        println!("{}", style("  Graph Stack Runner").cyan().bold());
        println!("{}", style("  ═══════════════════").cyan());
        println!();
    }

    if let Some(path) = env_path {
        out.info(&format!("Loaded env from {}", path.display()));
    }

    // Load configuration
    out.header("Configuration");

    // Find workspace root (where graph-stack-config lives)
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|p| p.join("graph-stack-config").exists())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    let config_path = args
        .config
        .unwrap_or_else(|| workspace_root.join("graph-stack-config/graph-stack.yaml"));

    std::env::set_current_dir(&workspace_root)?;

    let config = StackConfig::from_file(&config_path).map_err(|e| anyhow::anyhow!(e))?;
    let expected_services = config.services.len();

    out.ok(&format!("Stack: {}", config.name));
    out.info(&format!(
        "Tasks: {}",
        if config.tasks.is_empty() {
            "(none)".to_string()
        } else {
            config.tasks.keys().cloned().collect::<Vec<_>>().join(", ")
        }
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
    let endpoint: SocketAddr = format!("127.0.0.1:{}", args.port).parse()?;
    let builder = BaseDaemon::builder(config).with_endpoint(endpoint);

    let spinner = out.spinner("Creating daemon...");
    let daemon = GraphTestDaemon::from_builder(builder)
        .await
        .context("Failed to create daemon")?;
    spinner.finish_ok("Daemon created");

    // Start daemon (WebSocket server)
    daemon.start().await.context("Failed to start daemon")?;
    out.ok(&format!("WebSocket server on {}", endpoint));

    // Launch the stack
    out.header("Launching Stack");
    let spinner = out.spinner("Starting services...");

    let event_receiver = daemon
        .launch_stack()
        .await
        .context("Failed to launch stack")?;

    // Process events until all services started or timeout
    let mut tasks_completed = Vec::new();
    let mut services_started = Vec::new();
    let mut failures = Vec::new();

    let deadline = std::time::Instant::now() + Duration::from_secs(60);

    while std::time::Instant::now() < deadline {
        async_io::Timer::after(Duration::from_millis(100)).await;

        while let Ok(event) = event_receiver.try_recv() {
            match event {
                harness_core::daemon::DaemonEvent::Task(state) => {
                    let name = format!("{:?}", state);
                    if let Some(task_name) = extract_name(&name) {
                        if name.contains("Start") {
                            spinner.set_message(&format!("Task: {}...", task_name));
                        } else if name.contains("Completed") {
                            tasks_completed.push(task_name);
                        } else if name.contains("Failed") {
                            failures.push(format!("task:{}", task_name));
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
                        } else if name.contains("Failed") {
                            failures.push(format!("service:{}", svc_name));
                        }
                    }
                }
            }
        }

        // Check if we've started all expected services
        if services_started.len() + failures.len() >= expected_services {
            break;
        }
    }

    if failures.is_empty() {
        spinner.finish_ok(&format!(
            "Launched {} tasks, {} services",
            tasks_completed.len(),
            services_started.len()
        ));
    } else {
        spinner.finish_warn(&format!(
            "Launched {} services, {} failed",
            services_started.len(),
            failures.len()
        ));
        for failure in &failures {
            out.error(&format!("Failed: {}", failure));
        }
    }

    // Health checks
    out.header("Health Checks");
    let spinner = out.spinner("Checking service health...");
    async_io::Timer::after(Duration::from_secs(2)).await;

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
            spinner.finish_warn("No health check results");
        }
        Err(e) => {
            spinner.finish_err(&format!("Health check error: {}", e));
        }
    }

    // Show allocated ports
    out.header("Service Endpoints");
    let port_registry = daemon.base.service_manager().port_registry();
    for (service_name, ports) in &port_registry {
        for (port_name, port) in ports {
            out.info(&format!("{}.{}: localhost:{}", service_name, port_name, port));
        }
    }

    // Ready
    out.header("Ready");
    if !args.plain {
        println!();
        println!("  Stack is running. Press {} to shutdown.", style("Ctrl+C").yellow().bold());
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
    let spinner = out.spinner("Stopping services...");
    daemon.stop().await.context("Failed to stop daemon")?;
    spinner.finish_ok("Stack stopped");

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
