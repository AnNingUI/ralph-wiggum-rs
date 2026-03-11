pub mod status_bar;
pub mod tui;

pub use tui::{RalphTui, TuiInputAction};

use anyhow::Result;

use ralph_core::event::AgentEvent;
use ralph_core::progress::ProgressSnapshot;
use ralph_core::render::RenderLine;
use ralph_core::status::StatusMeta;

/// TUI-based output sink. Wraps `RalphTui` to implement `OutputSink`.
///
/// When constructed, the TUI takes over the terminal. All events and render
/// lines are routed to the ratatui interface. The status footer is auto-updated
/// from `ProgressSnapshot` on each event.
pub struct TuiOutput {
    inner: RalphTui,
}

impl TuiOutput {
    pub fn new(meta: StatusMeta) -> Result<Self> {
        let inner = RalphTui::new(meta)?;
        Ok(Self { inner })
    }

    pub fn tui(&mut self) -> &mut RalphTui {
        &mut self.inner
    }

    pub fn handle_input(&mut self) -> Result<TuiInputAction> {
        self.inner.handle_input()
    }

    pub fn finish(self) -> Result<()> {
        self.inner.finish()
    }
}

impl ralph_core::plugin::OutputSink for TuiOutput {
    fn emit_stdout(&mut self, line: &str) -> Result<()> {
        self.inner
            .push_render_line(RenderLine::status(line))
    }

    fn emit_stderr(&mut self, line: &str) -> Result<()> {
        self.inner
            .push_render_line(RenderLine::error(line))
    }

    fn render_line(&mut self, line: &RenderLine) -> Result<()> {
        self.inner.push_render_line(line.clone())
    }

    fn on_event(
        &mut self,
        _event: &AgentEvent,
        snapshot: &ProgressSnapshot,
    ) -> Result<()> {
        // Build a footer string from the snapshot for the status panel
        let footer = build_snapshot_footer(snapshot);
        self.inner.set_runtime(Some(snapshot.clone()), footer)?;
        Ok(())
    }

    fn set_status(&mut self, status: Option<String>) -> Result<()> {
        self.inner.set_footer(status)
    }

    fn set_meta(&mut self, meta: &StatusMeta) -> Result<()> {
        self.inner.set_meta(meta.clone())
    }

    fn check_interrupt(&mut self) -> Result<bool> {
        match self.inner.handle_input()? {
            TuiInputAction::ExitRequested => Ok(true),
            TuiInputAction::Continue => Ok(false),
        }
    }
}

/// Build a short footer string from the progress snapshot.
fn build_snapshot_footer(snapshot: &ProgressSnapshot) -> Option<String> {
    if !snapshot.phase.should_show_status() {
        return None;
    }

    let mut parts = Vec::new();

    if let Some(header) = snapshot
        .status_header
        .as_deref()
        .map(str::trim)
        .filter(|h| !h.is_empty())
    {
        parts.push(header.to_string());
    } else {
        parts.push(format!("{}", snapshot.phase.label()));
    }

    if snapshot.tool_calls > 0 {
        parts.push(format!("tools {}", snapshot.tool_calls));
    }

    if let Some(last_tool) = snapshot.last_tool.as_deref().filter(|t| !t.is_empty()) {
        parts.push(format!("last {last_tool}"));
    }

    if let Some(detail) = snapshot
        .last_detail
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
    {
        parts.push(ralph_core::status::shorten_middle(detail, 40));
    }

    Some(parts.join(" \u{00b7} "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ralph_core::progress::Phase;

    #[test]
    fn footer_returns_none_for_idle_phase() {
        let snapshot = ProgressSnapshot {
            phase: Phase::Idle,
            ..Default::default()
        };
        assert!(build_snapshot_footer(&snapshot).is_none());
    }

    #[test]
    fn footer_includes_header_and_tools() {
        let snapshot = ProgressSnapshot {
            phase: Phase::ToolExec,
            status_header: Some("Testing code".into()),
            tool_calls: 5,
            last_tool: Some("shell".into()),
            ..Default::default()
        };
        let footer = build_snapshot_footer(&snapshot).unwrap();
        assert!(footer.contains("Testing code"));
        assert!(footer.contains("tools 5"));
        assert!(footer.contains("last shell"));
    }

    #[test]
    fn footer_uses_phase_label_when_no_header() {
        let snapshot = ProgressSnapshot {
            phase: Phase::Thinking,
            ..Default::default()
        };
        let footer = build_snapshot_footer(&snapshot).unwrap();
        assert!(footer.contains("thinking"));
    }
}
