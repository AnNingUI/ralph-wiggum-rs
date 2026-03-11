use serde::Serialize;
use std::collections::HashMap;

use crate::event::{AgentEvent, McpStatus, ToolSource, ToolStatus};

/// Agent-agnostic progress snapshot. Aggregated from `AgentEvent` stream by `ProgressTracker`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ProgressSnapshot {
    pub phase: Phase,
    pub session_id: Option<String>,
    pub status_header: Option<String>,
    pub tool_calls: usize,
    pub active_tools: Vec<ActiveTool>,
    pub last_tool: Option<String>,
    pub last_detail: Option<String>,
    pub last_error: Option<String>,
    pub input_tokens: Option<i64>,
    pub cached_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub todo_completed: usize,
    pub todo_total: usize,
    pub mcp_servers: McpStartupSummary,
    pub loop_iteration: Option<u32>,
    pub loop_max: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Idle,
    Starting,
    Thinking,
    Planning,
    Reasoning,
    ToolExec,
    Responding,
    Completed,
    Failed,
}

impl Phase {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Starting => "starting",
            Self::Thinking => "thinking",
            Self::Planning => "planning",
            Self::Reasoning => "reasoning",
            Self::ToolExec => "tool",
            Self::Responding => "responding",
            Self::Completed => "done",
            Self::Failed => "failed",
        }
    }

    pub fn should_show_status(&self) -> bool {
        matches!(
            self,
            Self::Starting | Self::Thinking | Self::Planning | Self::Reasoning | Self::ToolExec
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActiveTool {
    pub call_id: String,
    pub tool: String,
    pub source: ToolSource,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct McpStartupSummary {
    pub ready: usize,
    pub starting: usize,
    pub failed: usize,
}

/// Consumes `AgentEvent` stream and maintains the latest `ProgressSnapshot`.
#[derive(Debug)]
pub struct ProgressTracker {
    snapshot: ProgressSnapshot,
    active_tool_calls: HashMap<String, ActiveTool>,
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressTracker {
    pub fn new() -> Self {
        Self {
            snapshot: ProgressSnapshot::default(),
            active_tool_calls: HashMap::new(),
        }
    }

    pub fn with_loop_info(mut self, iteration: u32, max: u32) -> Self {
        self.snapshot.loop_iteration = Some(iteration);
        self.snapshot.loop_max = Some(max);
        self
    }

    pub fn snapshot(&self) -> &ProgressSnapshot {
        &self.snapshot
    }

    pub fn observe(&mut self, event: &AgentEvent) {
        match event {
            AgentEvent::SessionStarted { session_id } => {
                self.snapshot.session_id = session_id.clone();
                self.snapshot.phase = Phase::Starting;
                self.snapshot.status_header = Some("Booting agent".to_string());
            }
            AgentEvent::TurnStarted { .. } => {
                self.snapshot.phase = self.phase_after_work();
                self.snapshot.status_header = Some("Working".to_string());
                self.snapshot.last_error = None;
            }
            AgentEvent::TurnComplete => {
                self.snapshot.phase = Phase::Completed;
                self.snapshot.status_header = Some("Completed".to_string());
            }

            AgentEvent::TextDelta { role, .. } => {
                if matches!(role, crate::event::Role::Assistant) {
                    self.snapshot.phase = Phase::Responding;
                    self.snapshot.status_header = Some("Writing response".to_string());
                }
            }
            AgentEvent::TextComplete { text, role } => {
                if matches!(role, crate::event::Role::Assistant) {
                    self.snapshot.phase = Phase::Responding;
                    self.snapshot.last_detail = Some(preview_text(text, 48));
                }
            }
            AgentEvent::ReasoningDelta { text } => {
                self.snapshot.phase = Phase::Reasoning;
                self.snapshot.status_header =
                    extract_header(text).or_else(|| Some("Analyzing".to_string()));
                self.snapshot.last_detail = Some(preview_text(text, 48));
            }
            AgentEvent::PlanUpdate { plan } => {
                self.snapshot.phase = Phase::Planning;
                self.snapshot.status_header = Some("Planning".to_string());
                self.snapshot.last_detail = Some(preview_text(plan, 48));
                let (completed, total) = count_todo_items(plan);
                self.snapshot.todo_completed = completed;
                self.snapshot.todo_total = total;
            }

            AgentEvent::ToolCallBegin {
                call_id,
                tool,
                detail,
                source,
            } => {
                self.snapshot.phase = Phase::ToolExec;
                self.snapshot.tool_calls += 1;
                self.snapshot.last_tool = Some(tool.clone());
                self.snapshot.last_detail = detail.as_ref().map(|d| preview_text(d, 48));
                self.snapshot.last_error = None;

                let active = ActiveTool {
                    call_id: call_id.clone(),
                    tool: tool.clone(),
                    source: *source,
                    detail: detail.clone(),
                };
                self.active_tool_calls.insert(call_id.clone(), active);
                self.sync_active_tools();
            }
            AgentEvent::ToolCallOutputDelta { call_id, chunk, .. } => {
                if let Some(active) = self.active_tool_calls.get_mut(call_id) {
                    active.detail = Some(preview_text(chunk, 48));
                }
            }
            AgentEvent::ToolCallEnd {
                call_id,
                tool,
                status,
                ..
            } => {
                self.active_tool_calls.remove(call_id);
                self.sync_active_tools();

                self.snapshot.last_tool = Some(tool.clone());
                if matches!(status, ToolStatus::Failed | ToolStatus::Declined) {
                    self.snapshot.phase = Phase::Failed;
                    self.snapshot.last_error = Some(format!("{tool} {status:?}"));
                } else {
                    self.snapshot.phase = self.phase_after_work();
                    self.snapshot.last_error = None;
                }
            }

            AgentEvent::ApprovalRequired { command, .. } => {
                self.snapshot.status_header = Some("Waiting for approval".to_string());
                self.snapshot.last_detail = Some(preview_text(command, 48));
            }
            AgentEvent::ApprovalResolved { .. } => {
                self.snapshot.status_header = Some("Working".to_string());
            }

            AgentEvent::McpServerUpdate { status, .. } => match status {
                McpStatus::Starting => self.snapshot.mcp_servers.starting += 1,
                McpStatus::Ready => {
                    self.snapshot.mcp_servers.starting =
                        self.snapshot.mcp_servers.starting.saturating_sub(1);
                    self.snapshot.mcp_servers.ready += 1;
                }
                McpStatus::Failed => {
                    self.snapshot.mcp_servers.starting =
                        self.snapshot.mcp_servers.starting.saturating_sub(1);
                    self.snapshot.mcp_servers.failed += 1;
                }
                McpStatus::Cancelled => {
                    self.snapshot.mcp_servers.starting =
                        self.snapshot.mcp_servers.starting.saturating_sub(1);
                }
            },
            AgentEvent::McpStartupComplete { ready, failed } => {
                self.snapshot.mcp_servers = McpStartupSummary {
                    ready: ready.len(),
                    starting: 0,
                    failed: failed.len(),
                };
            }

            AgentEvent::TokenUpdate {
                input,
                cached,
                output,
            } => {
                if input.is_some() {
                    self.snapshot.input_tokens = *input;
                }
                if cached.is_some() {
                    self.snapshot.cached_tokens = *cached;
                }
                if output.is_some() {
                    self.snapshot.output_tokens = *output;
                }
            }

            AgentEvent::SubagentSpawned { .. } => {}
            AgentEvent::SubagentComplete { .. } => {}

            AgentEvent::LoopIterationAdvanced { iteration } => {
                self.snapshot.loop_iteration = Some(*iteration);
            }
            AgentEvent::LoopOutcome { .. } => {}

            AgentEvent::ContextCompacted => {
                self.snapshot.status_header = Some("Context compacted".to_string());
            }
            AgentEvent::Error { message } => {
                self.snapshot.phase = Phase::Failed;
                self.snapshot.status_header = Some("Error".to_string());
                self.snapshot.last_error = Some(preview_text(message, 48));
            }
        }
    }

    fn phase_after_work(&self) -> Phase {
        if self.snapshot.todo_total > 0 && self.snapshot.todo_completed < self.snapshot.todo_total {
            Phase::Planning
        } else {
            Phase::Thinking
        }
    }

    fn sync_active_tools(&mut self) {
        self.snapshot.active_tools = self.active_tool_calls.values().cloned().collect();
    }
}

fn preview_text(text: &str, max_chars: usize) -> String {
    let single_line = text.replace(['\r', '\n'], " ");
    let trimmed = single_line.trim();
    if trimmed.chars().count() <= max_chars {
        trimmed.to_string()
    } else {
        let mut preview: String = trimmed.chars().take(max_chars).collect();
        preview.push('\u{2026}');
        preview
    }
}

fn extract_header(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(inner) = trimmed
            .strip_prefix("**")
            .and_then(|s| s.strip_suffix("**"))
        {
            let inner = inner.trim();
            if !inner.is_empty() {
                return Some(inner.to_string());
            }
        }
        return Some(preview_text(trimmed, 48));
    }
    None
}

fn count_todo_items(plan: &str) -> (usize, usize) {
    let mut completed = 0;
    let mut total = 0;

    for line in plan.lines() {
        let mut trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix('-') {
            trimmed = rest.trim_start();
        }
        let Some(rest) = trimmed.strip_prefix('[') else {
            continue;
        };
        let mut chars = rest.chars();
        let Some(marker) = chars.next() else {
            continue;
        };
        let Some(close) = chars.next() else {
            continue;
        };
        if close != ']' {
            continue;
        }
        total += 1;
        if marker.eq_ignore_ascii_case(&'x') {
            completed += 1;
        }
    }

    (completed, total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Role, ToolSource};

    #[test]
    fn tracker_advances_phase_on_tool_call() {
        let mut tracker = ProgressTracker::new();

        tracker.observe(&AgentEvent::ToolCallBegin {
            call_id: "c1".into(),
            tool: "shell".into(),
            detail: Some("ls".into()),
            source: ToolSource::Agent,
        });
        assert_eq!(tracker.snapshot().phase, Phase::ToolExec);
        assert_eq!(tracker.snapshot().tool_calls, 1);
        assert_eq!(tracker.snapshot().active_tools.len(), 1);

        tracker.observe(&AgentEvent::ToolCallEnd {
            call_id: "c1".into(),
            tool: "shell".into(),
            status: ToolStatus::Completed,
            duration_ms: Some(100),
            exit_code: Some(0),
        });
        assert_eq!(tracker.snapshot().phase, Phase::Thinking);
        assert!(tracker.snapshot().active_tools.is_empty());
    }

    #[test]
    fn tracker_aggregates_tokens() {
        let mut tracker = ProgressTracker::new();
        tracker.observe(&AgentEvent::TokenUpdate {
            input: Some(1200),
            cached: Some(300),
            output: Some(50),
        });
        assert_eq!(tracker.snapshot().input_tokens, Some(1200));
        assert_eq!(tracker.snapshot().cached_tokens, Some(300));
        assert_eq!(tracker.snapshot().output_tokens, Some(50));
    }

    #[test]
    fn tracker_tracks_mcp_servers() {
        let mut tracker = ProgressTracker::new();
        tracker.observe(&AgentEvent::McpServerUpdate {
            server: "fs".into(),
            status: McpStatus::Starting,
        });
        assert_eq!(tracker.snapshot().mcp_servers.starting, 1);

        tracker.observe(&AgentEvent::McpServerUpdate {
            server: "fs".into(),
            status: McpStatus::Ready,
        });
        assert_eq!(tracker.snapshot().mcp_servers.ready, 1);
        assert_eq!(tracker.snapshot().mcp_servers.starting, 0);
    }

    #[test]
    fn tracker_responds_to_text_delta() {
        let mut tracker = ProgressTracker::new();
        tracker.observe(&AgentEvent::TextDelta {
            text: "hello".into(),
            role: Role::Assistant,
        });
        assert_eq!(tracker.snapshot().phase, Phase::Responding);
    }
}
