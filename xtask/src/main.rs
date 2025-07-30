mod ci;
mod docker;
mod test;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Development task runner for graph-network-harness")]
struct Args {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run CI checks
    Ci(ci::CiArgs),
    /// Run tests
    Test(test::TestArgs),
    /// Docker operations
    Docker(docker::DockerArgs),
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Run in smol runtime
    smol::block_on(async {
        match args.cmd {
            Command::Ci(args) => ci::run(args).await,
            Command::Test(args) => test::run(args).await,
            Command::Docker(args) => docker::run(args).await,
        }
    })
}
