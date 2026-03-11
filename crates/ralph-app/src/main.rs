mod cli;
mod cmd_clear;
mod cmd_run;
mod cmd_status;
mod cmd_stop;
mod cmd_version;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Status) => cmd_status::run_status(),
        Some(Commands::Stop) => cmd_stop::run_stop(),
        Some(Commands::Clear) => cmd_clear::run_clear(),
        Some(Commands::Version) => cmd_version::run_version(),
        None => cmd_run::run_main(cli).await,
    }
}
