//! Build status metrics text from ProgressSnapshot for the TUI header.

use ralph_core::progress::ProgressSnapshot;
use ralph_core::status::{format_token_count, shorten_middle};

/// Build the single-line metrics summary shown in the TUI header row.
pub fn build_metrics_text(snapshot: Option<&ProgressSnapshot>) -> String {
    let Some(snapshot) = snapshot else {
        return "phase booting".to_string();
    };

    let mut parts = Vec::new();
    parts.push(format!("phase {}", snapshot.phase.label()));
    parts.push(format!("tools {}", snapshot.tool_calls));

    if snapshot.todo_total > 0 {
        parts.push(format!(
            "todo {}/{}",
            snapshot.todo_completed, snapshot.todo_total
        ));
    }

    if let Some(last_tool) = snapshot.last_tool.as_deref().filter(|t| !t.is_empty()) {
        parts.push(format!("last {last_tool}"));
    }

    let mcp_total =
        snapshot.mcp_servers.ready + snapshot.mcp_servers.starting + snapshot.mcp_servers.failed;
    if mcp_total > 0 {
        parts.push(format!(
            "mcp {}/{} ready",
            snapshot.mcp_servers.ready, mcp_total
        ));
    }

    if let (Some(input), Some(cached), Some(output)) = (
        snapshot.input_tokens,
        snapshot.cached_tokens,
        snapshot.output_tokens,
    ) {
        parts.push(format!(
            "tok i{} c{} o{}",
            format_token_count(input),
            format_token_count(cached),
            format_token_count(output)
        ));
    }

    if let Some(session_id) = snapshot.session_id.as_deref().filter(|s| !s.is_empty()) {
        parts.push(format!("sid {}", shorten_middle(session_id, 8)));
    }

    parts.join(" \u{00b7} ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ralph_core::progress::{McpStartupSummary, Phase};

    #[test]
    fn metrics_text_shows_booting_when_none() {
        assert_eq!(build_metrics_text(None), "phase booting");
    }

    #[test]
    fn metrics_text_includes_phase_tools_tokens() {
        let snapshot = ProgressSnapshot {
            phase: Phase::Thinking,
            tool_calls: 3,
            last_tool: Some("shell".into()),
            input_tokens: Some(12_300),
            cached_tokens: Some(4_000),
            output_tokens: Some(512),
            ..Default::default()
        };
        let text = build_metrics_text(Some(&snapshot));
        assert!(text.contains("phase thinking"));
        assert!(text.contains("tools 3"));
        assert!(text.contains("last shell"));
        assert!(text.contains("tok i12.3k"));
    }

    #[test]
    fn metrics_text_includes_mcp_summary() {
        let snapshot = ProgressSnapshot {
            phase: Phase::ToolExec,
            mcp_servers: McpStartupSummary {
                ready: 3,
                starting: 0,
                failed: 1,
            },
            ..Default::default()
        };
        let text = build_metrics_text(Some(&snapshot));
        assert!(text.contains("mcp 3/4 ready"));
    }
}
