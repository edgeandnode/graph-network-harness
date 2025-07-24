use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod commands;

#[derive(Parser)]
#[command(name = "harness")]
#[command(about = "Graph Network Harness - Service orchestration tool")]
#[command(version)]
struct Cli {
    /// Configuration file path
    #[arg(short, long, global = true, default_value = "services.yaml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Validate configuration file
    Validate {
        /// Strict mode - fail on missing environment variables
        #[arg(short, long)]
        strict: bool,
    },

    /// Start services
    Start {
        /// Services to start (empty means all)
        services: Vec<String>,
    },

    /// Stop services
    Stop {
        /// Services to stop (empty means all)
        services: Vec<String>,
        
        /// Force stop even if dependents are running
        #[arg(short, long)]
        force: bool,
        
        /// Timeout in seconds to wait for services to stop
        #[arg(short, long)]
        timeout: Option<u64>,
    },

    /// Show service status
    Status,

    /// Daemon management commands
    Daemon {
        #[command(subcommand)]
        command: DaemonCommands,
    },
}

#[derive(Subcommand)]
enum DaemonCommands {
    /// Check daemon status
    Status,
}

fn main() -> Result<()> {
    smol::block_on(async {
        let cli = Cli::parse();

        match cli.command {
            Commands::Validate { strict } => commands::validate::run(&cli.config, strict).await,
            Commands::Start { services } => commands::start::run(&cli.config, services).await,
            Commands::Stop { services, force, timeout } => commands::stop::run(&cli.config, services, force, timeout).await,
            Commands::Status => commands::status::run(&cli.config).await,
            Commands::Daemon { command } => commands::daemon::run(command).await,
        }
    })
}
