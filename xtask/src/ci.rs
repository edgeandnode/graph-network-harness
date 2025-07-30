use anyhow::{bail, Result};
use clap::{Args, Subcommand};
use command_executor::{
    backends::local::LocalLauncher, Command, Launcher, ProcessEventType, ProcessHandle, Target,
};
use futures::StreamExt;

#[derive(Args)]
pub struct CiArgs {
    #[command(subcommand)]
    cmd: CiCommand,
}

#[derive(Subcommand)]
pub enum CiCommand {
    /// Run all CI checks
    All,
    /// Format check (read-only)
    #[command(name = "fmt-check")]
    FmtCheck,
    /// Clippy lints
    Clippy,
    /// Cargo deny check
    Deny,
    /// Run unit tests only (no features)
    UnitTests,
    /// Run all tests with all features
    IntegrationTests,
}

pub async fn run(args: CiArgs) -> Result<()> {
    match args.cmd {
        CiCommand::All => run_all().await,
        CiCommand::FmtCheck => run_fmt().await,
        CiCommand::Clippy => run_clippy().await,
        CiCommand::Deny => run_deny().await,
        CiCommand::UnitTests => run_unit_tests().await,
        CiCommand::IntegrationTests => run_integration_tests().await,
    }
}

async fn run_all() -> Result<()> {
    println!("Running all CI checks\n");

    // Format check
    println!("Checking code formatting...");
    run_fmt().await?;
    println!("Format check passed\n");

    // Clippy
    println!("Running clippy lints...");
    run_clippy().await?;
    println!("Clippy check passed\n");

    // Deny (if available)
    if cargo_deny_available().await {
        println!("Running cargo deny...");
        run_deny().await?;
        println!("Dependency check passed\n");
    }

    // Unit tests
    println!("Running unit tests (no features)...");
    run_unit_tests().await?;
    println!("Unit tests passed\n");

    // Integration tests
    println!("Running integration tests (all features)...");
    crate::docker::ensure_test_images().await?;
    run_integration_tests().await?;
    println!("Integration tests passed\n");

    println!("All CI checks passed!");
    Ok(())
}

async fn run_fmt() -> Result<()> {
    let success = run_cargo_command(vec!["fmt", "--all", "--", "--check"]).await?;
    if !success {
        bail!("Format check failed. Run 'cargo fmt --all' to fix.");
    }
    Ok(())
}

async fn run_clippy() -> Result<()> {
    let success = run_cargo_command(vec![
        "clippy",
        "--workspace",
        "--all-targets",
        "--all-features",
        "--",
        "-D",
        "warnings",
    ])
    .await?;
    if !success {
        bail!("Clippy check failed");
    }
    Ok(())
}

async fn run_deny() -> Result<()> {
    let success = run_cargo_command(vec!["deny", "check"]).await?;
    if !success {
        bail!("Cargo deny check failed");
    }
    Ok(())
}

async fn run_unit_tests() -> Result<()> {
    run_tests(vec!["--lib", "--bins"], None).await
}

async fn run_integration_tests() -> Result<()> {
    run_tests(vec!["--all-features"], Some("all-features")).await
}

async fn run_tests(extra_args: Vec<&str>, features_desc: Option<&str>) -> Result<()> {
    if let Some(desc) = features_desc {
        println!("  Features: {}", desc);
        println!("  This includes: docker-tests, ssh-tests, integration-tests");
    }
    println!();

    let launcher = LocalLauncher;
    let mut args = vec!["test", "--workspace"];
    args.extend(extra_args);
    args.extend(&["--", "--nocapture"]);

    let cmd = Command::builder("cargo").args(&args).build();
    let (mut events, mut handle) = launcher.launch(&Target::Command, cmd).await?;

    let mut test_failed = false;
    let mut failure_count = 0;

    while let Some(event) = events.next().await {
        match &event.event_type {
            ProcessEventType::Stdout | ProcessEventType::Stderr => {
                if let Some(data) = &event.data {
                    print!("{}", data);
                    if data.contains("FAILED") {
                        failure_count += 1;
                        test_failed = true;
                    }
                    if data.contains("test result: FAILED") {
                        test_failed = true;
                    }
                }
            }
            ProcessEventType::Started { pid } => {
                eprintln!("Test process started (PID: {})", pid);
            }
            ProcessEventType::Exited { code, signal } => match (code, signal) {
                (Some(0), _) if !test_failed => {
                    println!("\nAll tests passed");
                }
                (Some(code), _) => {
                    eprintln!("\nTests exited with code: {}", code);
                }
                (_, Some(sig)) => {
                    eprintln!("\nTests terminated by signal: {}", sig);
                }
                _ => {
                    eprintln!("\nTests exited abnormally");
                }
            },
        }
    }

    let status = handle.wait().await?;

    if !status.success() || test_failed {
        bail!("Tests failed ({} failures)", failure_count);
    }

    Ok(())
}

async fn run_cargo_command(args: Vec<&str>) -> Result<bool> {
    let launcher = LocalLauncher;
    let cmd = Command::builder("cargo").args(args).build();

    let (mut events, mut handle) = launcher.launch(&Target::Command, cmd).await?;

    while let Some(event) = events.next().await {
        match &event.event_type {
            ProcessEventType::Stdout | ProcessEventType::Stderr => {
                if let Some(data) = &event.data {
                    print!("{}", data);
                }
            }
            _ => {}
        }
    }

    let status = handle.wait().await?;
    Ok(status.success())
}

async fn cargo_deny_available() -> bool {
    let launcher = LocalLauncher;
    let cmd = Command::builder("cargo")
        .args(&["deny", "--version"])
        .build();

    if let Ok((mut events, mut handle)) = launcher.launch(&Target::Command, cmd).await {
        // Drain events
        while events.next().await.is_some() {}

        if let Ok(status) = handle.wait().await {
            return status.success();
        }
    }
    false
}
