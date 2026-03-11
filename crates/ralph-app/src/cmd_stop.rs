//! Stop subcommand - interrupt the current loop.

use anyhow::Result;
use colored::Colorize;

use ralph_core::state::{load_state, state_exists};

pub fn run_stop() -> Result<()> {
    if !state_exists() {
        println!("{}", "No active loop to stop".yellow());
        return Ok(());
    }

    let state = load_state()?;

    if state.iteration == 0 {
        println!("{}", "No active loop to stop".yellow());
        return Ok(());
    }

    println!("{}", "Loop interrupted".green());
    println!("State preserved at iteration {}", state.iteration);
    println!("Use {} to continue", "ralph --resume".cyan());

    Ok(())
}
