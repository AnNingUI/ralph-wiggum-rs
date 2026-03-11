//! Status subcommand - show current loop state.

use anyhow::Result;
use colored::Colorize;

use ralph_core::state::{load_history, load_state, state_exists};
use ralph_core::truncate_string;

pub fn run_status() -> Result<()> {
    println!("{}", "Ralph Wiggum Loop Status".cyan().bold());
    println!();

    if !state_exists() {
        println!("{}", "No active loop".yellow());
        return Ok(());
    }

    let state = load_state()?;
    let history = load_history()?;

    if state.iteration == 0 {
        println!("{}", "No active loop".yellow());
        return Ok(());
    }

    println!("  Iteration:      {}/{}", state.iteration, state.max_iterations);
    println!(
        "  Prompt:         {}",
        truncate_string(&state.prompt, 60)
    );

    if let Some(promise) = &state.promise {
        println!("  Promise:        {}", truncate_string(promise, 60));
    }

    if let Some(tasks_file) = &state.tasks_file {
        println!("  Tasks file:     {}", tasks_file);
    }

    if state.one_session {
        println!("  One session:    enabled");
    }

    if let Some(session) = &state.codex_resume_session {
        println!("  Codex session:  {}", session);
    }

    println!();
    println!("  Total iterations:  {}", history.iterations.len());
    println!("  Total duration:    {}ms", history.total_duration_ms);

    Ok(())
}
