//! Universal agent process execution engine.
//!
//! Spawns the agent CLI, reads stdout/stderr, routes through Runner/RunContext/OutputSink.

use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::signal;
use tokio::time::{MissedTickBehavior, interval};

use ralph_core::options::AgentOptions;
use ralph_core::plugin::{OutputSink, OutputState, RunContext, Runner};
use ralph_core::progress::ProgressTracker;
use ralph_core::state::{clear_last_message_capture, load_last_message_capture};

/// Result of a single agent execution.
#[derive(Debug)]
pub struct ExecutionResult {
    pub output_buffer: String,
    pub tools_used: HashMap<String, u32>,
    pub latest_ai_response: Option<String>,
    pub session_id: Option<String>,
    pub exit_code: i32,
    pub interrupted: bool,
}

/// Run one agent invocation from spawn to completion.
///
/// The caller provides:
/// - `runner`: Agent-specific Runner (from AgentPlugin.create_runner)
/// - `prompt` / `model` / `options`: Execution parameters
/// - `sink`: OutputSink (PlainOutput, TuiOutput, etc.)
/// - `progress`: ProgressTracker (maintained across the call)
/// - `project_dir`: Working directory
pub async fn run_agent_once(
    runner: &mut dyn Runner,
    prompt: &str,
    model: &str,
    options: &AgentOptions,
    sink: &mut dyn OutputSink,
    progress: &mut ProgressTracker,
    project_dir: &Path,
) -> Result<ExecutionResult> {
    let mut output = OutputState::default();

    // Pre-spawn hook
    {
        let mut ctx = RunContext {
            output: &mut output,
            sink,
            progress,
            project_dir,
        };
        runner.before_spawn(prompt, &mut ctx)?;
    }

    let output_capture_path = runner.output_capture_path();
    if output_capture_path.is_some() {
        clear_last_message_capture()?;
    }

    let prepared_prompt = runner.prepare_prompt(prompt);
    let args = runner.build_args(&prepared_prompt, model, options);
    let env = runner.build_env(options);

    let mut cmd = Command::new(runner.command());
    cmd.args(&args)
        .envs(&env)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    if runner.wants_stdin_prompt() {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }

    let mut child = cmd.spawn()?;

    // Write prompt to stdin if needed
    if runner.wants_stdin_prompt() {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Failed to capture stdin"))?;
        stdin.write_all(prepared_prompt.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.shutdown().await?;
    }

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

    let ctrl_c = signal::ctrl_c();
    tokio::pin!(ctrl_c);

    // Optional tick interval for status updates
    let mut status_tick = runner.tick_interval().map(|duration| {
        let mut tick = interval(duration);
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        tick
    });
    if let Some(tick) = status_tick.as_mut() {
        tick.tick().await; // consume the immediate first tick
    }

    // Always poll for interrupt to keep the TUI responsive even when output stalls.
    let mut interrupt_tick = interval(std::time::Duration::from_millis(50));
    interrupt_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    interrupt_tick.tick().await;

    let mut stdout_closed = false;
    let mut stderr_closed = false;

    loop {
        // Check for user interrupt (ESC in TUI, etc.)
        if sink.check_interrupt()? {
            let _ = child.kill().await;
            return Ok(ExecutionResult {
                output_buffer: output.output_buffer,
                tools_used: output.tools_used,
                latest_ai_response: output.latest_ai_response,
                session_id: output.session_id,
                exit_code: -1,
                interrupted: true,
            });
        }

        // Check if process has exited
        let _ = child.try_wait();

        // If both streams are closed, we're done
        if stdout_closed && stderr_closed {
            break;
        }

        let has_tick = status_tick.is_some();

        tokio::select! {
            biased;

            _ = &mut ctrl_c => {
                let _ = child.kill().await;
                return Ok(ExecutionResult {
                    output_buffer: output.output_buffer,
                    tools_used: output.tools_used,
                    latest_ai_response: output.latest_ai_response,
                    session_id: output.session_id,
                    exit_code: -1,
                    interrupted: true,
                });
            }

            _ = interrupt_tick.tick() => {
                if sink.check_interrupt()? {
                    let _ = child.kill().await;
                    return Ok(ExecutionResult {
                        output_buffer: output.output_buffer,
                        tools_used: output.tools_used,
                        latest_ai_response: output.latest_ai_response,
                        session_id: output.session_id,
                        exit_code: -1,
                        interrupted: true,
                    });
                }
            }

            line = stdout_reader.next_line(), if !stdout_closed => {
                match line {
                    Ok(Some(line)) => {
                        let mut ctx = RunContext {
                            output: &mut output,
                            sink,
                            progress,
                            project_dir,
                        };
                        runner.handle_stdout(&line, &mut ctx)?;
                    }
                    Ok(None) => stdout_closed = true,
                    Err(e) => {
                        let message = format!("Error reading stdout: {e}");
                        let mut ctx = RunContext {
                            output: &mut output,
                            sink,
                            progress,
                            project_dir,
                        };
                        runner.handle_stderr(&message, &mut ctx)?;
                        stdout_closed = true;
                    }
                }
            }
            line = stderr_reader.next_line(), if !stderr_closed => {
                match line {
                    Ok(Some(line)) => {
                        let mut ctx = RunContext {
                            output: &mut output,
                            sink,
                            progress,
                            project_dir,
                        };
                        runner.handle_stderr(&line, &mut ctx)?;
                    }
                    Ok(None) => stderr_closed = true,
                    Err(e) => {
                        let message = format!("Error reading stderr: {e}");
                        let mut ctx = RunContext {
                            output: &mut output,
                            sink,
                            progress,
                            project_dir,
                        };
                        runner.handle_stderr(&message, &mut ctx)?;
                        stderr_closed = true;
                    }
                }
            }
            _ = async {
                if let Some(tick) = status_tick.as_mut() {
                    tick.tick().await;
                }
            }, if has_tick => {
                let mut ctx = RunContext {
                    output: &mut output,
                    sink,
                    progress,
                    project_dir,
                };
                runner.handle_tick(&mut ctx)?;
            }
        }
    }

    // Finish hook
    {
        let mut ctx = RunContext {
            output: &mut output,
            sink,
            progress,
            project_dir,
        };
        runner.finish(&mut ctx)?;
    }

    // Wait for process to exit and get exit code
    let exit_status = child.wait().await?;
    let exit_code = exit_status.code().unwrap_or(-1);

    // Resolve the latest AI response
    let handler_latest = output
        .latest_ai_response
        .take()
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty());

    let captured_latest = if output_capture_path.is_some() {
        load_last_message_capture()?
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
    } else {
        None
    };

    let latest_ai_response = runner.resolve_latest_response(
        exit_code,
        &output.output_buffer,
        handler_latest,
        captured_latest,
    );

    Ok(ExecutionResult {
        output_buffer: output.output_buffer,
        tools_used: output.tools_used,
        latest_ai_response,
        session_id: output.session_id,
        exit_code,
        interrupted: false,
    })
}
