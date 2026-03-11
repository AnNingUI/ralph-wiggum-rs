//! Clear subcommand - clear loop state and history.

use anyhow::Result;
use colored::Colorize;

use ralph_core::state::{clear_history, clear_state};

pub fn run_clear() -> Result<()> {
    clear_state()?;
    clear_history()?;

    println!("{}", "Loop state and history cleared".green());

    Ok(())
}
