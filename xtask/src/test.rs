use anyhow::{bail, Result};
use clap::Args;
use command_executor::{
    backends::local::LocalLauncher, Command, Launcher, ProcessEventType, ProcessHandle, Target,
};
use futures::StreamExt;

#[derive(Args)]
pub struct TestArgs {
    /// Package to test
    #[arg(short, long)]
    package: Option<String>,

    /// Features to enable
    #[arg(short, long)]
    features: Option<String>,

    /// Run all features
    #[arg(long)]
    all_features: bool,

    /// Test name filter
    filter: Option<String>,
}

pub async fn run(args: TestArgs) -> Result<()> {
    println!("Running tests\n");

    let launcher = LocalLauncher;
    let mut cmd_args = vec!["test"];

    // Add package if specified
    if let Some(package) = &args.package {
        cmd_args.push("-p");
        cmd_args.push(package);
    } else {
        cmd_args.push("--workspace");
    }

    // Add features
    if args.all_features {
        cmd_args.push("--all-features");
    } else if let Some(features) = &args.features {
        cmd_args.push("--features");
        cmd_args.push(features);
    }

    // Add test name filter
    if let Some(filter) = &args.filter {
        cmd_args.push("--");
        cmd_args.push(filter);
        cmd_args.push("--nocapture");
    } else {
        cmd_args.push("--");
        cmd_args.push("--nocapture");
    }

    // Show what we're running
    println!("Command: cargo {}", cmd_args.join(" "));
    if args.all_features {
        println!("Features: all-features (includes docker-tests, ssh-tests, integration-tests)");
    } else if let Some(features) = &args.features {
        println!("Features: {}", features);
    }
    println!();

    // Ensure Docker images if needed
    if args.all_features
        || args
            .features
            .as_ref()
            .map_or(false, |f| f.contains("docker") || f.contains("ssh"))
    {
        crate::docker::ensure_test_images().await?;
    }

    let cmd = Command::builder("cargo").args(&cmd_args).build();
    let (mut events, mut handle) = launcher.launch(&Target::Command, cmd).await?;

    let mut test_failed = false;
    let mut test_summary = TestSummary::default();

    while let Some(event) = events.next().await {
        match &event.event_type {
            ProcessEventType::Stdout | ProcessEventType::Stderr => {
                if let Some(data) = &event.data {
                    print!("{}", data);

                    // Parse test output
                    test_summary.parse_line(data);

                    if data.contains("test result: FAILED") || data.contains("FAILED") {
                        test_failed = true;
                    }
                }
            }
            ProcessEventType::Started { pid } => {
                eprintln!("Test process started (PID: {})", pid);
            }
            ProcessEventType::Exited { .. } => {
                // Handled after loop
            }
        }
    }

    let status = handle.wait().await?;

    // Print summary
    println!("\n{}", test_summary);

    if !status.success() || test_failed {
        bail!("Tests failed");
    }

    Ok(())
}

#[derive(Default)]
struct TestSummary {
    total: usize,
    passed: usize,
    failed: usize,
    ignored: usize,
}

impl TestSummary {
    fn parse_line(&mut self, line: &str) {
        if line.contains(" test") && line.contains(" ... ") {
            self.total += 1;
            if line.contains(" ... ok") {
                self.passed += 1;
            } else if line.contains(" ... FAILED") {
                self.failed += 1;
            } else if line.contains(" ... ignored") {
                self.ignored += 1;
            }
        }
    }
}

impl std::fmt::Display for TestSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.total > 0 {
            write!(
                f,
                "Test Summary: {} total, {} passed, {} failed, {} ignored",
                self.total, self.passed, self.failed, self.ignored
            )
        } else {
            write!(f, "No test results captured")
        }
    }
}
