//! Claude ralph-plugin loop state management.

use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ralph_core::completion::strip_ansi;

pub const RALPH_LOOP_STATE_FILE: &str = "ralph-loop.local.md";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeLoopOutcome {
    PromiseDetected(String),
    MaxIterations(u32),
    Warning(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeLoopEvent {
    IterationAdvanced(u32),
    Outcome(ClaudeLoopOutcome),
}

#[derive(Debug, Clone)]
pub struct ClaudeLoopState {
    pub active: bool,
    pub iteration: u32,
    pub max_iterations: u32,
    pub completion_promise: Option<String>,
    pub started_at: Option<String>,
    pub prompt: Option<String>,
}

pub fn ralph_state_path(project_dir: &Path) -> PathBuf {
    project_dir.join(".claude").join(RALPH_LOOP_STATE_FILE)
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

pub fn detect_outcome(output: &str) -> Option<ClaudeLoopOutcome> {
    let cleaned = strip_ansi(output);
    for line in cleaned.lines().rev() {
        if let Some(ClaudeLoopEvent::Outcome(outcome)) = parse_event(line) {
            return Some(outcome);
        }
    }
    None
}

pub fn parse_event(line: &str) -> Option<ClaudeLoopEvent> {
    let cleaned = strip_ansi(line);
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(iteration) = extract_iteration_marker(trimmed) {
        return Some(ClaudeLoopEvent::IterationAdvanced(iteration));
    }

    if !trimmed.contains("Ralph loop:") {
        return None;
    }

    if let Some(promise) = extract_promise(trimmed) {
        return Some(ClaudeLoopEvent::Outcome(ClaudeLoopOutcome::PromiseDetected(
            promise,
        )));
    }
    if let Some(max) = extract_max_iterations(trimmed) {
        return Some(ClaudeLoopEvent::Outcome(ClaudeLoopOutcome::MaxIterations(
            max,
        )));
    }
    Some(ClaudeLoopEvent::Outcome(ClaudeLoopOutcome::Warning(
        trimmed.to_string(),
    )))
}

pub fn read_ralph_state_file(project_dir: &Path) -> Result<Option<ClaudeLoopState>> {
    let state_path = ralph_state_path(project_dir);
    if !state_path.exists() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(&state_path)?;
    Ok(parse_ralph_state(&contents))
}

fn extract_promise(line: &str) -> Option<String> {
    let start_tag = "<promise>";
    let end_tag = "</promise>";
    let start = line.find(start_tag)? + start_tag.len();
    let end = line[start..].find(end_tag)? + start;
    let promise = line[start..end].trim();
    (!promise.is_empty()).then(|| promise.to_string())
}

fn extract_max_iterations(line: &str) -> Option<u32> {
    let open = line.find('(')?;
    let close = line[open + 1..].find(')')? + open + 1;
    let value = line[open + 1..close].trim();
    value.parse::<u32>().ok()
}

fn extract_iteration_marker(line: &str) -> Option<u32> {
    let needle = "Ralph iteration";
    let index = line.find(needle)?;
    let tail = line[index + needle.len()..].trim_start();
    let digits: String = tail
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u32>().ok()
}

fn parse_ralph_state(contents: &str) -> Option<ClaudeLoopState> {
    let (frontmatter, prompt) = parse_frontmatter(contents)?;

    let active = frontmatter
        .get("active")
        .and_then(|v| parse_bool_value(v))
        .unwrap_or(true);
    let iteration = frontmatter
        .get("iteration")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    let max_iterations = frontmatter
        .get("max_iterations")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    let completion_promise = frontmatter
        .get("completion_promise")
        .and_then(|v| parse_yaml_value(v));
    let started_at = frontmatter
        .get("started_at")
        .and_then(|v| parse_yaml_value(v));
    let prompt = prompt
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    Some(ClaudeLoopState {
        active,
        iteration,
        max_iterations,
        completion_promise,
        started_at,
        prompt,
    })
}

fn parse_frontmatter(contents: &str) -> Option<(HashMap<String, String>, Option<String>)> {
    let mut frontmatter_lines = Vec::new();
    let mut body_lines = Vec::new();
    let mut in_frontmatter = false;
    let mut frontmatter_done = false;

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            if !in_frontmatter {
                in_frontmatter = true;
                continue;
            }
            if !frontmatter_done {
                frontmatter_done = true;
                continue;
            }
        }

        if in_frontmatter && !frontmatter_done {
            frontmatter_lines.push(line);
        } else {
            body_lines.push(line);
        }
    }

    if !in_frontmatter {
        return None;
    }

    let mut map = HashMap::new();
    for line in frontmatter_lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            map.insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    let body = if body_lines.is_empty() {
        None
    } else {
        Some(body_lines.join("\n"))
    };

    Some((map, body))
}

fn parse_bool_value(value: &str) -> Option<bool> {
    match value.trim().to_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn parse_yaml_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("null") {
        return None;
    }
    let stripped = trimmed
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(trimmed);
    Some(stripped.replace("\\\"", "\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_outcome_parses_promise() {
        let output = "Ralph loop: Detected <promise>DONE</promise>";
        assert_eq!(
            detect_outcome(output),
            Some(ClaudeLoopOutcome::PromiseDetected("DONE".to_string()))
        );
    }

    #[test]
    fn detect_outcome_parses_max_iterations() {
        let output = "Ralph loop: Max iterations (12) reached.";
        assert_eq!(
            detect_outcome(output),
            Some(ClaudeLoopOutcome::MaxIterations(12))
        );
    }

    #[test]
    fn parse_event_detects_iteration_marker() {
        let output =
            "\u{1f504} Ralph iteration 3 | No completion promise set - loop runs infinitely";
        assert_eq!(
            parse_event(output),
            Some(ClaudeLoopEvent::IterationAdvanced(3))
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
