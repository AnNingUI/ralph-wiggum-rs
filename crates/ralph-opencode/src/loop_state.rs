//! Opencode ralph-plugin loop state management.

use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ralph_core::completion::strip_ansi;

pub const RALPH_LOOP_STATE_FILE: &str = "ralph-loop.local.md";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpencodeLoopOutcome {
    PromiseDetected(String),
    MaxIterations(u32),
    Warning(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpencodeLoopEvent {
    IterationAdvanced(u32),
    Outcome(OpencodeLoopOutcome),
}

#[derive(Debug, Clone)]
pub struct OpencodeLoopState {
    pub active: bool,
    pub iteration: u32,
    pub max_iterations: u32,
    pub completion_promise: Option<String>,
    pub started_at: Option<String>,
    pub prompt: Option<String>,
}

pub fn ralph_state_path(project_dir: &Path) -> PathBuf {
    project_dir.join(".opencode").join(RALPH_LOOP_STATE_FILE)
}

pub fn write_ralph_state_file(
    project_dir: &Path,
    prompt: &str,
    iteration: u32,
    max_iterations: u32,
    completion_promise: Option<&str>,
) -> Result<PathBuf> {
    let state_path = ralph_state_path(project_dir);
    if let Some(parent) = state_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let promise_value = completion_promise
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| format!("\"{}\"", v.replace('"', "\\\"")))
        .unwrap_or_else(|| "null".to_string());

    let started_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let contents = format!(
        "---\nactive: true\niteration: {iteration}\nmax_iterations: {max_iterations}\ncompletion_promise: {promise_value}\nstarted_at: \"{started_at}\"\n---\n\n{prompt}\n"
    );

    std::fs::write(&state_path, contents)?;
    Ok(state_path)
}

pub fn clear_ralph_state_file(project_dir: &Path) -> Result<Option<u32>> {
    let state_path = ralph_state_path(project_dir);
    if !state_path.exists() {
        return Ok(None);
    }
    let state = read_ralph_state_file(project_dir).unwrap_or(None);
    let iteration = state.as_ref().map(|s| s.iteration);
    std::fs::remove_file(&state_path)?;
    Ok(iteration)
}

pub fn detect_outcome(output: &str) -> Option<OpencodeLoopOutcome> {
    let cleaned = strip_ansi(output);
    for line in cleaned.lines().rev() {
        if let Some(OpencodeLoopEvent::Outcome(outcome)) = parse_event(line) {
            return Some(outcome);
        }
    }
    None
}

pub fn parse_event(line: &str) -> Option<OpencodeLoopEvent> {
    let cleaned = strip_ansi(line);
    let trimmed = cleaned.trim();

    // Iteration marker: 🔄 Ralph iteration N | ...
    if trimmed.starts_with("🔄 Ralph iteration ")
        && let Some(num_str) = trimmed
            .strip_prefix("🔄 Ralph iteration ")
            .and_then(|rest| rest.split_whitespace().next())
        && let Ok(iteration) = num_str.parse::<u32>()
    {
        return Some(OpencodeLoopEvent::IterationAdvanced(iteration));
    }

    // Promise detected
    if trimmed.starts_with("Ralph loop: Completion promise detected: ") {
        let promise = trimmed
            .strip_prefix("Ralph loop: Completion promise detected: ")
            .unwrap_or("")
            .to_string();
        return Some(OpencodeLoopEvent::Outcome(
            OpencodeLoopOutcome::PromiseDetected(promise),
        ));
    }

    // Max iterations
    if trimmed.starts_with("Ralph loop: Max iterations (")
        && let Some(num_str) = trimmed
            .strip_prefix("Ralph loop: Max iterations (")
            .and_then(|rest| rest.split(')').next())
        && let Ok(max) = num_str.parse::<u32>()
    {
        return Some(OpencodeLoopEvent::Outcome(
            OpencodeLoopOutcome::MaxIterations(max),
        ));
    }

    None
}

pub fn read_ralph_state_file(project_dir: &Path) -> Result<Option<OpencodeLoopState>> {
    let state_path = ralph_state_path(project_dir);
    if !state_path.exists() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(&state_path)?;
    Ok(parse_ralph_state(&contents))
}

pub fn parse_ralph_state(contents: &str) -> Option<OpencodeLoopState> {
    let (frontmatter, body) = split_frontmatter(contents)?;
    let fields = parse_yaml_frontmatter(&frontmatter);

    let active = fields
        .get("active")
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(false);
    let iteration = fields
        .get("iteration")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    let max_iterations = fields
        .get("max_iterations")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    let completion_promise = fields
        .get("completion_promise")
        .filter(|v| *v != "null")
        .map(|v| v.trim_matches('"').to_string());
    let started_at = fields
        .get("started_at")
        .map(|v| v.trim_matches('"').to_string());
    let prompt = Some(body.trim().to_string()).filter(|v| !v.is_empty());

    Some(OpencodeLoopState {
        active,
        iteration,
        max_iterations,
        completion_promise,
        started_at,
        prompt,
    })
}

fn split_frontmatter(contents: &str) -> Option<(String, String)> {
    let trimmed = contents.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }
    let after_first = trimmed.strip_prefix("---")?;
    let end_idx = after_first.find("\n---\n")?;
    let frontmatter = after_first[..end_idx].to_string();
    let body = after_first[end_idx + 5..].to_string();
    Some((frontmatter, body))
}

fn parse_yaml_frontmatter(frontmatter: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    for line in frontmatter.lines() {
        if let Some((key, value)) = line.split_once(':') {
            fields.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_outcome_parses_promise() {
        let output = "Ralph loop: Completion promise detected: DONE";
        assert_eq!(
            detect_outcome(output),
            Some(OpencodeLoopOutcome::PromiseDetected("DONE".to_string()))
        );
    }

    #[test]
    fn detect_outcome_parses_max_iterations() {
        let output = "Ralph loop: Max iterations (12) reached.";
        assert_eq!(
            detect_outcome(output),
            Some(OpencodeLoopOutcome::MaxIterations(12))
        );
    }

    #[test]
    fn parse_event_detects_iteration_marker() {
        let output =
            "\u{1f504} Ralph iteration 3 | No completion promise set - loop runs infinitely";
        assert_eq!(
            parse_event(output),
            Some(OpencodeLoopEvent::IterationAdvanced(3))
        );
    }

    #[test]
    fn parse_ralph_state_reads_frontmatter() {
        let contents = "---\nactive: true\niteration: 2\nmax_iterations: 5\ncompletion_promise: \"DONE\"\nstarted_at: \"2025-01-01T00:00:00Z\"\n---\n\nFinish the task\n";
        let state = parse_ralph_state(contents).expect("should parse");
        assert_eq!(state.iteration, 2);
        assert_eq!(state.max_iterations, 5);
        assert_eq!(state.completion_promise.as_deref(), Some("DONE"));
        assert_eq!(state.prompt.as_deref(), Some("Finish the task"));
    }
}
