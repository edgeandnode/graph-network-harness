use anyhow::{bail, Result};
use clap::{Args, Subcommand};
use command_executor::{
    backends::local::LocalLauncher, Command, Launcher, ProcessEventType, ProcessHandle, Target,
};
use futures::StreamExt;
use std::env;

#[derive(Args)]
pub struct DockerArgs {
    #[command(subcommand)]
    cmd: DockerCommand,
}

#[derive(Subcommand)]
pub enum DockerCommand {
    /// Build test container images
    BuildTestImages,
    /// Clean test container images
    CleanTestImages,
}

pub async fn run(args: DockerArgs) -> Result<()> {
    match args.cmd {
        DockerCommand::BuildTestImages => build_test_images().await,
        DockerCommand::CleanTestImages => clean_test_images().await,
    }
}

pub async fn ensure_test_images() -> Result<()> {
    println!("Checking Docker test images...");

    let images = vec![(
        "command-executor-test-systemd:latest",
        "crates/command-executor/tests/systemd-container",
        "Dockerfile",
    )];

    for (image_name, context_dir, dockerfile) in images {
        print!("  Checking for {}... ", image_name);

        if docker_image_exists(image_name).await? {
            println!("found");
        } else {
            println!("not found");

            if env::var("GITHUB_ACTIONS").is_ok() {
                println!("  Running in GitHub Actions, attempting to use build cache");
            }

            println!("  Building {} from {}", image_name, dockerfile);
            build_docker_image(image_name, context_dir, dockerfile).await?;
            println!("  Built {} successfully", image_name);
        }
    }

    println!("Test images ready\n");
    Ok(())
}

async fn build_test_images() -> Result<()> {
    println!("Building test container images...\n");

    let images = vec![(
        "command-executor-test-systemd:latest",
        "crates/command-executor/tests/systemd-container",
        "Dockerfile",
    )];

    for (image_name, context_dir, dockerfile) in images {
        println!("Building {}...", image_name);
        build_docker_image(image_name, context_dir, dockerfile).await?;
        println!("Built {} successfully\n", image_name);
    }

    Ok(())
}

async fn clean_test_images() -> Result<()> {
    println!("Cleaning test container images...\n");

    let images = vec!["command-executor-test-systemd:latest"];

    for image_name in images {
        println!("Removing {}...", image_name);
        remove_docker_image(image_name).await?;
    }

    println!("Test images cleaned");
    Ok(())
}

async fn docker_image_exists(image: &str) -> Result<bool> {
    let launcher = LocalLauncher;
    let cmd = Command::builder("docker")
        .args(["images", "-q", image])
        .build();

    let (mut events, mut handle) = launcher.launch(&Target::Command, cmd).await?;

    let mut has_output = false;

    while let Some(event) = events.next().await {
        if event.event_type == ProcessEventType::Stdout {
            if let Some(data) = &event.data {
                if !data.trim().is_empty() {
                    has_output = true;
                }
            }
        }
    }

    let status = handle.wait().await?;
    Ok(status.success() && has_output)
}

async fn build_docker_image(image_name: &str, context_dir: &str, dockerfile: &str) -> Result<()> {
    let launcher = LocalLauncher;

    let mut args = vec!["build", "-t", image_name, "-f"];
    args.push(dockerfile);

    // Add caching in GitHub Actions
    if env::var("GITHUB_ACTIONS").is_ok() {
        args.extend(&["--cache-from", "type=gha"]);
        args.extend(&["--cache-to", "type=gha,mode=max"]);
    }

    args.push(context_dir);

    let cmd = Command::builder("docker").args(&args).build();
    let (mut events, mut handle) = launcher.launch(&Target::Command, cmd).await?;

    while let Some(event) = events.next().await {
        match &event.event_type {
            ProcessEventType::Stdout | ProcessEventType::Stderr => {
                if let Some(data) = &event.data {
                    println!("{}", data);
                }
            }
            _ => {}
        }
    }

    let status = handle.wait().await?;
    if !status.success() {
        bail!("Failed to build Docker image {}", image_name);
    }

    Ok(())
}

async fn remove_docker_image(image: &str) -> Result<()> {
    let launcher = LocalLauncher;
    let cmd = Command::builder("docker").args(["rmi", image]).build();

    let (mut events, mut handle) = launcher.launch(&Target::Command, cmd).await?;

    while let Some(event) = events.next().await {
        match &event.event_type {
            ProcessEventType::Stdout | ProcessEventType::Stderr => {
                if let Some(data) = &event.data {
                    println!("{}", data);
                }
            }
            _ => {}
        }
    }

    let status = handle.wait().await?;
    if !status.success() {
        eprintln!("Warning: Failed to remove image {}", image);
    }

    Ok(())
}
