//! Ralph Wiggum iteration loop.
//!
//! Drives the outer loop: prompt building, agent execution, completion
//! detection, state persistence, and agent rotation.

use anyhow::Result;
use std::path::Path;
use std::time::Instant;
use tokio::time::{Duration, sleep};

use ralph_core::options::AgentOptions;
use ralph_core::plugin::{LoopMode, OutputSink, Runner};
use ralph_core::progress::ProgressTracker;
use ralph_core::{AgentEvent, render::RenderLine};
use ralph_core::state::{
    IterationHistory, RalphState, clear_state, load_context, load_history, load_prev_ai_response,
    save_history, save_prev_ai_response, save_state,
};
use ralph_core::{check_terminal_promise, inject_prev_ai_context, tasks_markdown_all_complete};

use crate::config::ResolvedModel;
use crate::exec::{self, ExecutionResult};
use crate::exec_streaming;

/// Parameters for one iteration of the loop.
pub struct IterationParams<'a> {
    pub runner: &'a mut dyn Runner,
    pub options: &'a AgentOptions,
    pub model: &'a ResolvedModel,
    pub agent_name: &'a str,
    pub sink: &'a mut dyn OutputSink,
    pub progress: &'a mut ProgressTracker,
    pub project_dir: &'a Path,
}

/// Result of the full loop run.
#[derive(Debug)]
pub struct LoopOutcome {
    pub completed: bool,
    pub total_iterations: u32,
    pub total_duration_ms: u64,
}

/// Run a single iteration: build prompt, execute agent, check completion.
pub async fn run_iteration(
    state: &mut RalphState,
    params: IterationParams<'_>,
) -> Result<IterationResult> {
    let iteration_start = Instant::now();

    // Build full prompt with context
    let mut full_prompt = state.prompt.clone();

    if let Some(context) = load_context()? {
        full_prompt = format!("{context}\n\n{full_prompt}");
    }

    if let Some(promise) = &state.promise {
        full_prompt = format!("{full_prompt}\n\nOutput <promise>{promise}</promise> when complete.");
    }

    if let Some(tasks_file) = &state.tasks_file {
        full_prompt = format!("{full_prompt}\n\nMark all tasks complete in {tasks_file}.");
    }

    let previous_ai_response = load_prev_ai_response()?;
    full_prompt = inject_prev_ai_context(&full_prompt, previous_ai_response.as_deref());

    // Execute agent - choose execution engine based on output format
    let execution = if params.runner.needs_streaming_json() {
        // Use streaming engine for stream-json format (Claude Code)
        exec_streaming::run_agent_streaming(
            params.runner,
            &full_prompt,
            &params.model.execution_model,
            params.options,
            params.sink,
            params.progress,
            params.project_dir,
        )
        .await?
    } else {
        // Use line-based engine for JSONL format (Codex)
        exec::run_agent_once(
            params.runner,
            &full_prompt,
            &params.model.execution_model,
            params.options,
            params.sink,
            params.progress,
            params.project_dir,
        )
        .await?
    };

    if execution.interrupted {
        return Ok(IterationResult {
            interrupted: true,
            completed: false,
            duration_ms: iteration_start.elapsed().as_millis() as u64,
            execution,
        });
    }

    // Save latest AI response
    if let Some(response) = execution.latest_ai_response.as_ref() {
        save_prev_ai_response(response)?;
    }

    // Build completion text
    let mut completion_output = execution.output_buffer.clone();
    if let Some(response) = execution.latest_ai_response.as_ref() {
        let trimmed = response.trim();
        if !trimmed.is_empty() && !completion_output.contains(trimmed) {
            if !completion_output.is_empty() {
                completion_output.push('\n');
            }
            completion_output.push_str(trimmed);
        }
    }

    // Check completion
    let mut completed = false;

    if let Some(promise) = &state.promise
        && check_terminal_promise(&completion_output, promise) {
            completed = true;
        }

    if let Some(tasks_file) = &state.tasks_file {
        let tasks_path = std::path::Path::new(tasks_file);
        let resolved_path = if tasks_path.is_absolute() {
            tasks_path.to_path_buf()
        } else {
            params.project_dir.join(tasks_path)
        };
        if let Ok(tasks_content) = std::fs::read_to_string(&resolved_path)
            && tasks_markdown_all_complete(&tasks_content) {
                completed = true;
            }
    }

    let loop_feedback = params.runner.loop_feedback(&completion_output);
    if loop_feedback.should_stop {
        completed = true;
    }

    Ok(IterationResult {
        interrupted: false,
        completed,
        duration_ms: iteration_start.elapsed().as_millis() as u64,
        execution,
    })
}

/// Result of a single iteration.
pub struct IterationResult {
    pub interrupted: bool,
    pub completed: bool,
    pub duration_ms: u64,
    pub execution: ExecutionResult,
}

/// Run the full Ralph Wiggum loop until completion or max iterations.
///
/// This is a high-level orchestration function. The caller is responsible for:
/// - Creating the AgentPlugin / Runner
/// - Setting up the OutputSink (PlainOutput or TuiOutput)
/// - Loading/saving RalphState for the initial prompt
pub async fn run_loop(
    state: &mut RalphState,
    create_runner: &mut dyn FnMut() -> Result<Box<dyn Runner>>,
    options: &AgentOptions,
    model: &ResolvedModel,
    agent_name: &str,
    sink: &mut dyn OutputSink,
    project_dir: &Path,
    delay_secs: u64,
    loop_mode: LoopMode,
) -> Result<LoopOutcome> {
    let mut history = load_history()?;

    emit_loop_start(sink, state)?;

    while should_continue(state.iteration, state.max_iterations) {
        emit_iteration_header(sink, state)?;

        let mut runner = create_runner()?;
        let mut progress = ProgressTracker::new()
            .with_loop_info(state.iteration, state.max_iterations);
        let loop_event = AgentEvent::LoopIterationAdvanced {
            iteration: state.iteration,
        };
        progress.observe(&loop_event);
        sink.on_event(&loop_event, progress.snapshot())?;

        let result = run_iteration(
            state,
            IterationParams {
                runner: runner.as_mut(),
                options,
                model,
                agent_name,
                sink,
                progress: &mut progress,
                project_dir,
            },
        )
        .await?;

        if result.interrupted {
            sink.render_line(&RenderLine::status("Interrupted by user"))?;
            sink.render_line(&RenderLine::status(
                "State preserved; use --resume to continue.",
            ))?;
            return Ok(LoopOutcome {
                completed: false,
                total_iterations: state.iteration,
                total_duration_ms: history.total_duration_ms,
            });
        }

        // Record iteration
        let mut errors = Vec::new();
        if result.execution.exit_code != 0 {
            errors.push(format!("Exit code: {}", result.execution.exit_code));
        }

        let record = IterationHistory {
            iteration: state.iteration,
            started_at: chrono::Utc::now().to_rfc3339(),
            ended_at: chrono::Utc::now().to_rfc3339(),
            duration_ms: result.duration_ms,
            agent: agent_name.to_string(),
            model: model.display_model.clone(),
            tools_used: result.execution.tools_used,
            files_modified: Vec::new(),
            exit_code: result.execution.exit_code,
            completion_detected: result.completed,
            errors,
        };
        history.iterations.push(record);
        history.total_duration_ms += result.duration_ms;
        save_history(&history)?;

        if result.completed {
            sink.render_line(&RenderLine::status("Task completed successfully!"))?;
            sink.render_line(&RenderLine::status(format!(
                "Total iterations: {}",
                state.iteration
            )))?;
            sink.render_line(&RenderLine::status(format!(
                "Total time: {}ms",
                history.total_duration_ms
            )))?;
            clear_state()?;
            return Ok(LoopOutcome {
                completed: true,
                total_iterations: state.iteration,
                total_duration_ms: history.total_duration_ms,
            });
        }

        // For internal loop mode, the agent handles its own looping
        if loop_mode == LoopMode::Internal {
            sink.render_line(&RenderLine::status(
                "Agent loop ended without completion.",
            ))?;
            clear_state()?;
            return Ok(LoopOutcome {
                completed: false,
                total_iterations: state.iteration,
                total_duration_ms: history.total_duration_ms,
            });
        }

        // Rotate agent if configured
        if let Some(rotation) = &state.rotation
            && !rotation.is_empty() {
                state.rotation_index =
                    Some((state.rotation_index.unwrap_or(0) + 1) % rotation.len());
            }

        state.iteration += 1;
        save_state(state)?;

        // Inter-iteration delay
        if should_continue(state.iteration, state.max_iterations) && delay_secs > 0 {
            sink.render_line(&RenderLine::status(format!(
                "Waiting {delay_secs}s before next iteration..."
            )))?;
            sink.set_status(Some(format!(
                "Waiting {delay_secs}s before next iteration..."
            )))?;

            let wait_start = Instant::now();
            while wait_start.elapsed() < Duration::from_secs(delay_secs) {
                sleep(Duration::from_millis(120)).await;
                if sink.check_interrupt()? {
                    sink.render_line(&RenderLine::status("Interrupted by user"))?;
                    sink.render_line(&RenderLine::status(
                        "State preserved; use --resume to continue.",
                    ))?;
                    return Ok(LoopOutcome {
                        completed: false,
                        total_iterations: state.iteration,
                        total_duration_ms: history.total_duration_ms,
                    });
                }
            }
            sink.set_status(None)?;
        }
    }

    // Max iterations reached
    sink.render_line(&RenderLine::status("Maximum iterations reached"))?;
    sink.render_line(&RenderLine::status(format!(
        "Total time: {}ms",
        history.total_duration_ms
    )))?;
    clear_state()?;

    Ok(LoopOutcome {
        completed: false,
        total_iterations: state.iteration.saturating_sub(1),
        total_duration_ms: history.total_duration_ms,
    })
}

fn should_continue(iteration: u32, max_iterations: u32) -> bool {
    max_iterations == 0 || iteration <= max_iterations
}

fn max_iterations_label(max: u32) -> String {
    if max == 0 {
        "unlimited".to_string()
    } else {
        max.to_string()
    }
}

fn emit_loop_start(sink: &mut dyn OutputSink, state: &RalphState) -> Result<()> {
    sink.render_line(&RenderLine::status("Starting Ralph Wiggum loop..."))?;
    sink.render_line(&RenderLine::status(format!("Prompt: {}", state.prompt)))?;
    sink.render_line(&RenderLine::status(format!(
        "Max iterations: {}",
        max_iterations_label(state.max_iterations)
    )))?;
    Ok(())
}

fn emit_iteration_header(sink: &mut dyn OutputSink, state: &RalphState) -> Result<()> {
    sink.render_line(&RenderLine::status(format!(
        "--- Iteration {}/{} ---",
        state.iteration,
        max_iterations_label(state.max_iterations)
    )))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_continue_respects_max() {
        assert!(should_continue(1, 10));
        assert!(should_continue(10, 10));
        assert!(!should_continue(11, 10));
    }

    #[test]
    fn should_continue_unlimited_when_zero() {
        assert!(should_continue(1, 0));
        assert!(should_continue(999, 0));
    }

    #[test]
    fn max_iterations_label_shows_unlimited() {
        assert_eq!(max_iterations_label(0), "unlimited");
        assert_eq!(max_iterations_label(10), "10");
    }
}
