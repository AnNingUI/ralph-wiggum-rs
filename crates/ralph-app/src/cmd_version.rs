//! Version subcommand - show version and build info.

use anyhow::Result;
use colored::Colorize;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run_version() -> Result<()> {
    println!("{} {}", "ralph-wiggum".cyan().bold(), VERSION);
    println!();
    println!("Modular architecture:");
    println!("  ralph-core    - Core types and traits");
    println!("  ralph-codex   - Codex agent plugin");
    println!("  ralph-claude  - Claude agent plugin");
    println!("  ralph-tui     - Terminal UI");
    println!("  ralph-cli     - CLI orchestration");
    println!("  ralph-app     - Binary entry point");

    Ok(())
}
