use colored::Colorize;
use std::io::Write as _;
use std::time::{Duration, Instant};

use crate::progress::ProgressSnapshot;

/// Agent-agnostic metadata shown alongside the status line.
#[derive(Debug, Clone)]
pub struct StatusMeta {
    pub agent: String,
    pub model: String,
    pub effort: String,
    pub project_path: String,
    pub iteration: u32,
    pub max_iterations: u32,
    pub started_at: Instant,
}

/// Agent-agnostic status renderer. Draws a single-line status bar on stderr.
/// Generalized from the Codex-specific `CodexStatusRenderer`.
#[derive(Debug)]
pub struct StatusRenderer {
    meta: StatusMeta,
    is_visible: bool,
    last_output_at: Instant,
}

impl StatusRenderer {
    pub fn new(meta: StatusMeta) -> Self {
        Self {
            meta,
            is_visible: false,
            last_output_at: Instant::now(),
        }
    }

    pub fn meta(&self) -> &StatusMeta {
        &self.meta
    }

    pub fn meta_mut(&mut self) -> &mut StatusMeta {
        &mut self.meta
    }

    fn clear_for_log_line(&mut self) {
        if self.is_visible {
            eprint!("\r\x1b[2K");
            let _ = std::io::stderr().flush();
            self.is_visible = false;
        }
    }

    pub fn note_output_activity(&mut self) {
        self.clear_for_log_line();
        self.last_output_at = Instant::now();
    }

    pub fn tick(&mut self, snapshot: &ProgressSnapshot) {
        self.redraw(snapshot);
    }

    pub fn finish(&mut self) {
        self.clear_for_log_line();
    }

    pub fn current_status_line(&self, snapshot: &ProgressSnapshot) -> Option<String> {
        if self.last_output_at.elapsed() < Duration::from_millis(450) {
            return None;
        }

        // MCP startup header takes priority
        if snapshot.mcp_servers.starting > 0 {
            let header = format!(
                "Starting MCP servers ({}/{})",
                snapshot.mcp_servers.ready,
                snapshot.mcp_servers.ready
                    + snapshot.mcp_servers.starting
                    + snapshot.mcp_servers.failed,
            );
            return Some(self.build_line_with_header(snapshot, &header));
        }

        if !snapshot.phase.should_show_status() {
            return None;
        }

        let header = snapshot
            .status_header
            .as_deref()
            .map(str::trim)
            .filter(|h| !h.is_empty())
            .unwrap_or("Working");

        Some(self.build_line_with_header(snapshot, header))
    }

    fn redraw(&mut self, snapshot: &ProgressSnapshot) {
        let Some(status) = self.current_status_line(snapshot) else {
            self.clear_for_log_line();
            return;
        };
        eprint!("\r\x1b[2K{}", status.dimmed());
        let _ = std::io::stderr().flush();
        self.is_visible = true;
    }

    fn build_line_with_header(&self, snapshot: &ProgressSnapshot, header: &str) -> String {
        let elapsed = format_duration(self.meta.started_at.elapsed().as_millis() as u64);
        let spinner = spinner_frame(self.meta.started_at.elapsed().as_millis());

        let mut priority_segments = Vec::new();
        let mut optional_segments = Vec::new();
        let mut token_segment = None;

        // token summary
        if let (Some(inp), Some(cached), Some(out)) = (
            snapshot.input_tokens,
            snapshot.cached_tokens,
            snapshot.output_tokens,
        ) {
            token_segment = Some(format!(
                "tok in {} | cached {} | out {}",
                format_token_count(inp),
                format_token_count(cached),
                format_token_count(out),
            ));
        }

        if snapshot.todo_total > 0 {
            optional_segments.push(format!(
                "todo {}/{}",
                snapshot.todo_completed, snapshot.todo_total
            ));
        }
        if snapshot.tool_calls > 0 {
            optional_segments.push(format!("tools {}", snapshot.tool_calls));
        }
        if let Some(last_tool) = snapshot.last_tool.as_deref().filter(|t| !t.is_empty()) {
            optional_segments.push(format!("last {last_tool}"));
        }
        if let Some(detail) = snapshot
            .last_detail
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty() && *d != header)
        {
            optional_segments.push(shorten_middle(detail, 28));
        }
        if let Some(err) = snapshot
            .last_error
            .as_deref()
            .map(str::trim)
            .filter(|e| !e.is_empty())
        {
            optional_segments.push(format!("err {}", shorten_middle(err, 20)));
        }

        // MCP summary
        let mcp_total = snapshot.mcp_servers.ready
            + snapshot.mcp_servers.starting
            + snapshot.mcp_servers.failed;
        if mcp_total > 0 && snapshot.mcp_servers.starting == 0 {
            optional_segments.push(format!(
                "mcp {}/{} ready",
                snapshot.mcp_servers.ready, mcp_total
            ));
        }

        priority_segments.push(format!("model {}", self.meta.model));
        if !self.meta.effort.is_empty() {
            priority_segments.push(format!("effort {}", self.meta.effort));
        }
        priority_segments.push(format!(
            "loop {}/{}",
            self.meta.iteration, self.meta.max_iterations
        ));
        priority_segments.push(format!(
            "path {}",
            shorten_middle(&self.meta.project_path, 12)
        ));
        if let Some(tok) = token_segment {
            priority_segments.push(tok);
        }

        let mut line = format!("{spinner} {header} ({elapsed}s \u{00b7} ctrl+c to interrupt)");
        let max_chars = status_max_chars();

        for segment in priority_segments
            .into_iter()
            .chain(optional_segments.into_iter())
        {
            let next_len = line.chars().count() + 3 + segment.chars().count();
            if next_len <= max_chars {
                line.push_str(" \u{00b7} ");
                line.push_str(&segment);
            }
        }

        if line.chars().count() > max_chars {
            shorten_middle(&line, max_chars)
        } else {
            line
        }
    }
}

pub fn format_duration(ms: u64) -> String {
    let seconds = ms / 1000;
    let minutes = seconds / 60;
    let hours = minutes / 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes % 60, seconds % 60)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds % 60)
    } else {
        format!("{seconds}")
    }
}

pub fn format_token_count(value: i64) -> String {
    match value {
        1_000_000.. => format!("{:.1}m", value as f64 / 1_000_000.0),
        10_000.. => format!("{:.1}k", value as f64 / 1_000.0),
        1_000.. => format!("{:.0}k", value as f64 / 1_000.0),
        _ => value.to_string(),
    }
}

pub fn shorten_middle(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars || max_chars <= 5 {
        return value.to_string();
    }
    let head = max_chars / 2;
    let tail = max_chars.saturating_sub(head + 3);
    let prefix: String = value.chars().take(head).collect();
    let suffix: String = value
        .chars()
        .rev()
        .take(tail)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{prefix}...{suffix}")
}

fn status_max_chars() -> usize {
    #[cfg(test)]
    {
        200
    }

    #[cfg(not(test))]
    {
        std::env::var("COLUMNS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .or_else(|| crossterm::terminal::size().ok().map(|(w, _)| w as usize))
            .map(|w| w.saturating_sub(6).clamp(52, 160))
            .unwrap_or(96)
    }
}

fn spinner_frame(elapsed_ms: u128) -> &'static str {
    const FRAMES: [&str; 4] = ["-", "\\", "|", "/"];
    FRAMES[((elapsed_ms / 120) as usize) % FRAMES.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_token_count_ranges() {
        assert_eq!(format_token_count(500), "500");
        assert_eq!(format_token_count(1_500), "2k");
        assert_eq!(format_token_count(12_300), "12.3k");
        assert_eq!(format_token_count(1_500_000), "1.5m");
    }

    #[test]
    fn shorten_middle_preserves_short_strings() {
        assert_eq!(shorten_middle("hello", 10), "hello");
    }

    #[test]
    fn shorten_middle_truncates_long_strings() {
        let result = shorten_middle("abcdefghijklmnop", 10);
        assert!(result.contains("..."));
        assert!(result.chars().count() <= 10);
    }

    #[test]
    fn status_renderer_builds_line() {
        let meta = StatusMeta {
            agent: "codex".into(),
            model: "gpt-5.4".into(),
            effort: "xhigh".into(),
            project_path: "D:/Dev-Project/demo".into(),
            iteration: 2,
            max_iterations: 5,
            started_at: Instant::now(),
        };
        let renderer = StatusRenderer::new(meta);
        let snapshot = ProgressSnapshot {
            phase: crate::progress::Phase::Thinking,
            status_header: Some("Testing code".into()),
            tool_calls: 3,
            input_tokens: Some(12_300),
            cached_tokens: Some(4_000),
            output_tokens: Some(512),
            ..Default::default()
        };

        let line = renderer.build_line_with_header(&snapshot, "Testing code");
        assert!(line.contains("Testing code"));
        assert!(line.contains("ctrl+c to interrupt"));
        assert!(line.contains("model gpt-5.4"));
    }
}
