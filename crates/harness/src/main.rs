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
    Validate,
    
    /// Start services
    Start {
        /// Services to start (empty means all)
        services: Vec<String>,
    },
    
    /// Stop services
    Stop {
        /// Services to stop (empty means all)
        services: Vec<String>,
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
        Commands::Validate => {
            commands::validate::run(&cli.config).await
        }
        Commands::Start { services } => {
            commands::start::run(&cli.config, services).await
        }
        Commands::Stop { services } => {
            commands::stop::run(&cli.config, services).await
        }
        Commands::Status => {
            commands::status::run(&cli.config).await
        }
        Commands::Daemon { command } => {
            commands::daemon::run(command).await
        }
    }
    })
}