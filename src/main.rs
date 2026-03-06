//! Ralph Wiggum Loop for AI agents
//!
//! Implementation of the Ralph Wiggum technique - continuous self-referential
//! AI loops for iterative development. Based on ghuntley.com/ralph/

use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use colored::*;
use std::collections::HashMap;
use std::process::Stdio;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::{Duration, sleep};

use ralph_wiggum_rs::{
    IterationHistory, RalphState,
    agent::{AgentBuildArgsOptions, AgentEnvOptions, AgentType, create_default_agent},
    check_terminal_promise,
    state::*,
    tasks_markdown_all_complete,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name = "ralph")]
#[command(version = VERSION)]
#[command(about = "Ralph Wiggum technique for iterative AI development loops", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// The prompt to send to the AI agent
    #[arg(value_name = "PROMPT")]
    prompt: Option<String>,

    /// Maximum number of iterations
    #[arg(short = 'n', long, default_value = "10")]
    max_iterations: u32,

    /// AI agent to use (opencode, claude-code, codex, copilot)
    #[arg(long, default_value = "opencode")]
    agent: String,

    /// Model to use
    #[arg(long, default_value = "claude-sonnet-4")]
    model: String,

    /// Promise tag for completion detection
    #[arg(long)]
    promise: Option<String>,

    /// Path to tasks markdown file for completion detection
    #[arg(long)]
    tasks: Option<String>,

    /// Enable agent rotation (comma-separated: agent1:model1,agent2:model2)
    #[arg(long)]
    rotation: Option<String>,

    /// Delay between iterations in seconds
    #[arg(long, default_value = "2")]
    delay: u64,

    /// Resume from existing state
    #[arg(long)]
    resume: bool,

    /// Additional context to inject
    #[arg(long)]
    context: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Show current loop status
    Status,
    /// Stop the current loop
    Stop,
    /// Clear all state
    Clear,
    /// Show version information
    Version,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Status) => {
            show_status()?;
            return Ok(());
        }
        Some(Commands::Stop) => {
            stop_loop()?;
            return Ok(());
        }
        Some(Commands::Clear) => {
            clear_state()?;
            println!("{}", "✓ Cleared all Ralph state".green());
            return Ok(());
        }
        Some(Commands::Version) => {
            println!("ralph-wiggum v{}", VERSION);
            return Ok(());
        }
        None => {}
    }

    // Main loop execution
    if cli.resume {
        if !state_exists() {
            return Err(anyhow!("No existing state to resume from"));
        }
        println!("{}", "🔄 Resuming from existing state...".cyan());
        run_ralph_loop(None, &cli).await?;
    } else {
        let prompt = cli
            .prompt
            .as_ref()
            .ok_or_else(|| anyhow!("Prompt is required"))?;

        if state_exists() {
            return Err(anyhow!(
                "A Ralph loop is already running. Use --resume to continue or 'ralph stop' to clear."
            ));
        }

        run_ralph_loop(Some(prompt.clone()), &cli).await?;
    }

    Ok(())
}

async fn run_ralph_loop(prompt: Option<String>, cli: &Cli) -> Result<()> {
    let mut state = if let Some(p) = prompt {
        let mut s = RalphState::new(p, cli.max_iterations);

        // Parse rotation if provided
        if let Some(rotation_str) = &cli.rotation {
            let pairs: Result<Vec<_>> = rotation_str
                .split(',')
                .map(|pair| {
                    let parts: Vec<&str> = pair.split(':').collect();
                    if parts.len() != 2 {
                        return Err(anyhow!("Invalid rotation format: {}", pair));
                    }
                    Ok(AgentModelPair {
                        agent: parts[0].to_string(),
                        model: parts[1].to_string(),
                    })
                })
                .collect();
            s.rotation = Some(pairs?);
            s.rotation_index = Some(0);
        }

        s.promise = cli.promise.clone();
        s.tasks_file = cli.tasks.clone();

        save_state(&s)?;
        s
    } else {
        load_state()?
    };

    let mut history = load_history()?;

    // Save context if provided
    if let Some(context) = &cli.context {
        save_context(context)?;
    }

    println!("{}", "🚀 Starting Ralph Wiggum loop...".green().bold());
    println!("{}", format!("   Prompt: {}", state.prompt).dimmed());
    println!(
        "{}",
        format!("   Max iterations: {}", state.max_iterations).dimmed()
    );
    println!();

    while state.iteration <= state.max_iterations {
        let iteration_start = Instant::now();

        println!(
            "{}",
            format!(
                "━━━ Iteration {}/{} ━━━",
                state.iteration, state.max_iterations
            )
            .cyan()
            .bold()
        );

        // Determine current agent and model
        let (current_agent, current_model) = if let Some(pair) = state.get_current_agent_model() {
            (pair.agent, pair.model)
        } else {
            (cli.agent.clone(), cli.model.clone())
        };

        println!(
            "{}",
            format!("   Agent: {} / {}", current_agent, current_model).dimmed()
        );

        // Build full prompt with context
        let mut full_prompt = state.prompt.clone();

        if let Some(context) = load_context()? {
            full_prompt = format!("{}\n\n{}", context, full_prompt);
        }

        if let Some(promise) = &state.promise {
            full_prompt = format!(
                "{}\n\nOutput <promise>{}</promise> when complete.",
                full_prompt, promise
            );
        }

        if let Some(tasks_file) = &state.tasks_file {
            full_prompt = format!(
                "{}\n\nMark all tasks complete in {}.",
                full_prompt, tasks_file
            );
        }

        // Execute agent
        let agent_type = AgentType::from_str(&current_agent)?;
        let agent = create_default_agent(agent_type);

        let args_options = AgentBuildArgsOptions {
            allow_all_permissions: false,
            extra_flags: Vec::new(),
            stream_output: true,
        };

        let env_options = AgentEnvOptions {
            filter_plugins: false,
            allow_all_permissions: false,
        };

        let args = agent.build_args(&full_prompt, &current_model, &args_options);
        let env = agent.build_env(&env_options);

        let mut cmd = Command::new(agent.command());
        cmd.args(&args)
            .envs(&env)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn()?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to capture stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("Failed to capture stderr"))?;

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut output_buffer = String::with_capacity(4096);
        let mut tools_used: HashMap<String, u32> = HashMap::new();

        // Read output streams
        loop {
            tokio::select! {
                line = stdout_reader.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            println!("{}", line);
                            output_buffer.push_str(&line);
                            output_buffer.push('\n');

                            if let Some(tool) = agent.parse_tool_output(&line) {
                                *tools_used.entry(tool).or_insert(0) += 1;
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            eprintln!("{}", format!("Error reading stdout: {}", e).red());
                            break;
                        }
                    }
                }
                line = stderr_reader.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            eprintln!("{}", line.dimmed());
                        }
                        Ok(None) => {}
                        Err(e) => {
                            eprintln!("{}", format!("Error reading stderr: {}", e).red());
                        }
                    }
                }
            }
        }

        let exit_status = child.wait().await?;
        let exit_code = exit_status.code().unwrap_or(-1);

        let iteration_duration = iteration_start.elapsed().as_millis() as u64;

        // Check completion
        let mut completion_detected = false;

        if let Some(promise) = &state.promise {
            if check_terminal_promise(&output_buffer, promise) {
                println!("{}", "✓ Promise detected!".green().bold());
                completion_detected = true;
            }
        }

        if let Some(_) = &state.tasks_file {
            if let Ok(Some(tasks_content)) = load_tasks() {
                if tasks_markdown_all_complete(&tasks_content) {
                    println!("{}", "✓ All tasks complete!".green().bold());
                    completion_detected = true;
                }
            }
        }

        // Record iteration
        let iteration_record = IterationHistory {
            iteration: state.iteration,
            started_at: chrono::Utc::now().to_rfc3339(),
            ended_at: chrono::Utc::now().to_rfc3339(),
            duration_ms: iteration_duration,
            agent: current_agent.clone(),
            model: current_model.clone(),
            tools_used,
            files_modified: Vec::new(),
            exit_code,
            completion_detected,
            errors: if exit_code != 0 {
                vec![format!("Exit code: {}", exit_code)]
            } else {
                Vec::new()
            },
        };

        history.iterations.push(iteration_record);
        history.total_duration_ms += iteration_duration;
        save_history(&history)?;

        if completion_detected {
            println!();
            println!("{}", "🎉 Task completed successfully!".green().bold());
            println!(
                "{}",
                format!("   Total iterations: {}", state.iteration).dimmed()
            );
            println!(
                "{}",
                format!("   Total time: {}ms", history.total_duration_ms).dimmed()
            );
            clear_state()?;
            return Ok(());
        }

        // Rotate agent if needed
        if let Some(rotation) = &state.rotation {
            if !rotation.is_empty() {
                state.rotation_index =
                    Some((state.rotation_index.unwrap_or(0) + 1) % rotation.len());
            }
        }

        state.iteration += 1;
        save_state(&state)?;

        if state.iteration <= state.max_iterations {
            println!();
            println!(
                "{}",
                format!("⏳ Waiting {}s before next iteration...", cli.delay).dimmed()
            );
            sleep(Duration::from_secs(cli.delay)).await;
            println!();
        }
    }

    println!();
    println!("{}", "⚠ Maximum iterations reached".yellow().bold());
    println!(
        "{}",
        format!("   Total time: {}ms", history.total_duration_ms).dimmed()
    );
    clear_state()?;

    Ok(())
}

fn show_status() -> Result<()> {
    if !state_exists() {
        println!("{}", "No active Ralph loop".dimmed());
        return Ok(());
    }

    let state = load_state()?;
    let history = load_history()?;

    println!("{}", "🔄 ACTIVE LOOP".cyan().bold());
    println!(
        "{}",
        format!(
            "   Iteration:    {} / {}",
            state.iteration, state.max_iterations
        )
    );
    println!("{}", format!("   Prompt:       {}", state.prompt));

    if let Some(rotation) = &state.rotation {
        println!();
        println!(
            "{}",
            format!(
                "   Rotation (position {}/{}):",
                state.rotation_index.unwrap_or(0) + 1,
                rotation.len()
            )
        );
        for (i, pair) in rotation.iter().enumerate() {
            let marker = if Some(i) == state.rotation_index {
                "**ACTIVE**"
            } else {
                ""
            };
            println!("   {}. {}:{}  {}", i + 1, pair.agent, pair.model, marker);
        }
    }

    if !history.iterations.is_empty() {
        println!();
        println!(
            "{}",
            format!("📊 HISTORY ({} iterations)", history.iterations.len())
                .cyan()
                .bold()
        );
        println!(
            "{}",
            format!("   Total time:   {}ms", history.total_duration_ms)
        );
        println!();
        println!("{}", "   Recent iterations:".dimmed());

        for iter in history.iterations.iter().rev().take(5) {
            let tools_summary: String = iter
                .tools_used
                .iter()
                .map(|(name, count)| format!("{}({})", name, count))
                .collect::<Vec<_>>()
                .join(" ");

            println!(
                "   #{}  {}ms  {} / {}  {}",
                iter.iteration, iter.duration_ms, iter.agent, iter.model, tools_summary
            );
        }
    }

    Ok(())
}

fn stop_loop() -> Result<()> {
    if !state_exists() {
        println!("{}", "No active Ralph loop to stop".dimmed());
        return Ok(());
    }

    clear_state()?;
    println!("{}", "✓ Stopped Ralph loop".green());
    Ok(())
}
