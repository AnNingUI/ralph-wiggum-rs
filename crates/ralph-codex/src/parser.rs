use serde::Deserialize;
use std::collections::HashMap;

use ralph_core::{
    AgentEvent, OutputStream, Role, ToolSource, ToolStatus,
    RenderLine,
};

// ── Raw Codex JSONL protocol types ──

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum CodexThreadEvent {
    #[serde(rename = "thread.started")]
    ThreadStarted { thread_id: String },
    #[serde(rename = "turn.started")]
    TurnStarted {},
    #[serde(rename = "turn.completed")]
    TurnCompleted { usage: Option<CodexUsage> },
    #[serde(rename = "turn.failed")]
    TurnFailed { error: CodexThreadError },
    #[serde(rename = "item.started")]
    ItemStarted { item: CodexThreadItem },
    #[serde(rename = "item.updated")]
    ItemUpdated { item: CodexThreadItem },
    #[serde(rename = "item.completed")]
    ItemCompleted { item: CodexThreadItem },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CodexUsage {
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CodexThreadError {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CodexThreadItem {
    pub id: String,
    #[serde(flatten)]
    pub details: CodexThreadItemDetails,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CodexThreadItemDetails {
    AgentMessage {
        text: String,
    },
    Reasoning {
        text: String,
    },
    CommandExecution {
        command: String,
        #[serde(default)]
        aggregated_output: String,
        exit_code: Option<i32>,
        status: CodexCommandExecutionStatus,
    },
    FileChange {
        #[serde(default)]
        changes: Vec<CodexFileUpdateChange>,
        status: CodexPatchApplyStatus,
    },
    McpToolCall {
        server: String,
        tool: String,
        status: CodexToolCallStatus,
        error: Option<CodexErrorItem>,
    },
    CollabToolCall {
        tool: CodexCollabTool,
        status: CodexToolCallStatus,
    },
    WebSearch {
        id: String,
        query: String,
    },
    TodoList {
        items: Vec<CodexTodoItem>,
    },
    Error {
        message: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexCommandExecutionStatus {
    InProgress,
    Completed,
    Failed,
    Declined,
}

impl std::fmt::Display for CodexCommandExecutionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InProgress => f.write_str("in_progress"),
            Self::Completed => f.write_str("completed"),
            Self::Failed => f.write_str("failed"),
            Self::Declined => f.write_str("declined"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CodexFileUpdateChange {
    pub path: String,
    pub kind: CodexPatchChangeKind,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexPatchApplyStatus {
    InProgress,
    Completed,
    Failed,
}

impl std::fmt::Display for CodexPatchApplyStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InProgress => f.write_str("in_progress"),
            Self::Completed => f.write_str("completed"),
            Self::Failed => f.write_str("failed"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexPatchChangeKind {
    Add,
    Delete,
    Update,
}

impl std::fmt::Display for CodexPatchChangeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Add => f.write_str("A"),
            Self::Delete => f.write_str("D"),
            Self::Update => f.write_str("M"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexToolCallStatus {
    InProgress,
    Completed,
    Failed,
}

impl std::fmt::Display for CodexToolCallStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InProgress => f.write_str("in_progress"),
            Self::Completed => f.write_str("completed"),
            Self::Failed => f.write_str("failed"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexCollabTool {
    SpawnAgent,
    SendInput,
    Wait,
    CloseAgent,
}

impl std::fmt::Display for CodexCollabTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SpawnAgent => f.write_str("spawn_agent"),
            Self::SendInput => f.write_str("send_input"),
            Self::Wait => f.write_str("wait"),
            Self::CloseAgent => f.write_str("close_agent"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CodexErrorItem {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CodexTodoItem {
    pub text: String,
    pub completed: bool,
}

// ── Event parser: CodexThreadEvent → AgentEvent + RenderLine ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemLifecycle {
    Started,
    Updated,
    Completed,
}

#[derive(Debug, Default)]
pub struct CodexEventParser {
    command_output_lengths: HashMap<String, usize>,
    todo_snapshots: HashMap<String, String>,
}

/// Result of parsing a single JSONL line.
pub struct ParseResult {
    pub events: Vec<AgentEvent>,
    pub lines: Vec<RenderLine>,
    /// If the agent produced a complete text response, its content.
    pub output_buffer_text: Option<String>,
    /// Tool name observed in this line.
    pub tool_name: Option<String>,
}

impl CodexEventParser {
    pub fn parse_line(&mut self, line: &str) -> Result<ParseResult, serde_json::Error> {
        let raw: CodexThreadEvent = serde_json::from_str(line)?;
        Ok(self.convert(raw))
    }

    fn convert(&mut self, raw: CodexThreadEvent) -> ParseResult {
        match raw {
            CodexThreadEvent::ThreadStarted { thread_id } => ParseResult {
                events: vec![AgentEvent::SessionStarted {
                    session_id: Some(thread_id.clone()),
                }],
                lines: vec![RenderLine::status(format!(
                    "[codex] thread started: {thread_id}"
                ))],
                output_buffer_text: None,
                tool_name: None,
            },
            CodexThreadEvent::TurnStarted {} => ParseResult {
                events: vec![AgentEvent::TurnStarted { turn_id: None }],
                lines: vec![RenderLine::status("[codex] turn started")],
                output_buffer_text: None,
                tool_name: None,
            },
            CodexThreadEvent::TurnCompleted { usage } => {
                let mut events: Vec<AgentEvent> = Vec::new();
                if let Some(u) = &usage {
                    events.push(AgentEvent::TokenUpdate {
                        input: Some(u.input_tokens),
                        cached: Some(u.cached_input_tokens),
                        output: Some(u.output_tokens),
                    });
                }
                events.push(AgentEvent::TurnComplete);

                let line_text = usage
                    .as_ref()
                    .map(|u| {
                        format!(
                            "[codex] turn completed \u{00b7} in={} cached={} out={}",
                            u.input_tokens, u.cached_input_tokens, u.output_tokens
                        )
                    })
                    .unwrap_or_else(|| "[codex] turn completed".to_string());

                ParseResult {
                    events,
                    lines: vec![RenderLine::status(line_text)],
                    output_buffer_text: None,
                    tool_name: None,
                }
            }
            CodexThreadEvent::TurnFailed { error } => ParseResult {
                events: vec![AgentEvent::Error {
                    message: error.message.clone(),
                }],
                lines: vec![RenderLine::error(format!(
                    "[codex] turn failed: {}",
                    error.message
                ))],
                output_buffer_text: None,
                tool_name: None,
            },
            CodexThreadEvent::Error { message } => ParseResult {
                events: vec![AgentEvent::Error {
                    message: message.clone(),
                }],
                lines: vec![RenderLine::error(format!("[codex] error: {message}"))],
                output_buffer_text: None,
                tool_name: None,
            },
            CodexThreadEvent::ItemStarted { item } => {
                self.convert_item(item, ItemLifecycle::Started)
            }
            CodexThreadEvent::ItemUpdated { item } => {
                self.convert_item(item, ItemLifecycle::Updated)
            }
            CodexThreadEvent::ItemCompleted { item } => {
                self.convert_item(item, ItemLifecycle::Completed)
            }
            CodexThreadEvent::Unknown => ParseResult {
                events: vec![],
                lines: vec![],
                output_buffer_text: None,
                tool_name: None,
            },
        }
    }

    fn convert_item(&mut self, item: CodexThreadItem, lifecycle: ItemLifecycle) -> ParseResult {
        match item.details {
            CodexThreadItemDetails::AgentMessage { text } => {
                if matches!(lifecycle, ItemLifecycle::Completed) {
                    ParseResult {
                        events: vec![AgentEvent::TextComplete {
                            text: text.clone(),
                            role: Role::Assistant,
                        }],
                        lines: text.lines().map(|l| RenderLine::assistant(l)).collect(),
                        output_buffer_text: Some(text),
                        tool_name: None,
                    }
                } else {
                    ParseResult {
                        events: vec![],
                        lines: vec![],
                        output_buffer_text: None,
                        tool_name: None,
                    }
                }
            }

            CodexThreadItemDetails::Reasoning { text } => ParseResult {
                events: vec![AgentEvent::ReasoningDelta { text: text.clone() }],
                lines: text.lines().map(|l| RenderLine::reasoning(l)).collect(),
                output_buffer_text: None,
                tool_name: None,
            },

            CodexThreadItemDetails::CommandExecution {
                command,
                aggregated_output,
                exit_code,
                status,
            } => self.convert_command_execution(
                &item.id,
                lifecycle,
                &command,
                &aggregated_output,
                exit_code,
                &status,
            ),

            CodexThreadItemDetails::McpToolCall {
                server,
                tool,
                status,
                error,
            } => self.convert_mcp_tool_call(&item.id, lifecycle, &server, &tool, &status, error),

            CodexThreadItemDetails::CollabToolCall { tool, status } => {
                self.convert_collab_tool_call(&item.id, lifecycle, &tool, &status)
            }

            CodexThreadItemDetails::WebSearch { id: _, query } => {
                self.convert_web_search(&item.id, lifecycle, &query)
            }

            CodexThreadItemDetails::FileChange { changes, status } => {
                self.convert_file_change(&item.id, lifecycle, &changes, &status)
            }

            CodexThreadItemDetails::TodoList { items } => {
                self.convert_todo_list(&item.id, lifecycle, &items)
            }

            CodexThreadItemDetails::Error { message } => ParseResult {
                events: vec![AgentEvent::Error {
                    message: message.clone(),
                }],
                lines: vec![RenderLine::error(format!("[codex:item] {message}"))],
                output_buffer_text: None,
                tool_name: None,
            },

            CodexThreadItemDetails::Unknown => ParseResult {
                events: vec![],
                lines: vec![],
                output_buffer_text: None,
                tool_name: None,
            },
        }
    }

    fn convert_command_execution(
        &mut self,
        item_id: &str,
        lifecycle: ItemLifecycle,
        command: &str,
        aggregated_output: &str,
        exit_code: Option<i32>,
        status: &CodexCommandExecutionStatus,
    ) -> ParseResult {
        let mut events = Vec::new();
        let mut lines = Vec::new();

        match lifecycle {
            ItemLifecycle::Started => {
                events.push(AgentEvent::ToolCallBegin {
                    call_id: item_id.to_string(),
                    tool: "shell".to_string(),
                    detail: Some(command.to_string()),
                    source: ToolSource::Agent,
                });
                lines.push(RenderLine::tool_call(format!("[tool:shell] {command}")));

                if let Some(delta) = self.take_output_delta(item_id, aggregated_output) {
                    events.push(AgentEvent::ToolCallOutputDelta {
                        call_id: item_id.to_string(),
                        stream: OutputStream::Stdout,
                        chunk: delta.clone(),
                    });
                    lines.extend(delta.lines().map(|l| RenderLine::tool_output_delta(l)));
                }

                ParseResult {
                    events,
                    lines,
                    output_buffer_text: None,
                    tool_name: Some("shell".to_string()),
                }
            }
            ItemLifecycle::Updated => {
                if let Some(delta) = self.take_output_delta(item_id, aggregated_output) {
                    events.push(AgentEvent::ToolCallOutputDelta {
                        call_id: item_id.to_string(),
                        stream: OutputStream::Stdout,
                        chunk: delta.clone(),
                    });
                    lines.extend(delta.lines().map(|l| RenderLine::tool_output_delta(l)));
                }
                ParseResult {
                    events,
                    lines,
                    output_buffer_text: None,
                    tool_name: None,
                }
            }
            ItemLifecycle::Completed => {
                if let Some(delta) = self.take_output_delta(item_id, aggregated_output) {
                    events.push(AgentEvent::ToolCallOutputDelta {
                        call_id: item_id.to_string(),
                        stream: OutputStream::Stdout,
                        chunk: delta.clone(),
                    });
                    lines.extend(delta.lines().map(|l| RenderLine::tool_output_delta(l)));
                }

                let tool_status = match status {
                    CodexCommandExecutionStatus::Completed => ToolStatus::Completed,
                    CodexCommandExecutionStatus::Failed => ToolStatus::Failed,
                    CodexCommandExecutionStatus::Declined => ToolStatus::Declined,
                    CodexCommandExecutionStatus::InProgress => ToolStatus::Completed,
                };

                events.push(AgentEvent::ToolCallEnd {
                    call_id: item_id.to_string(),
                    tool: "shell".to_string(),
                    status: tool_status,
                    duration_ms: None,
                    exit_code,
                });

                let summary = match exit_code {
                    Some(code) => {
                        format!("[tool:shell:{status}] {command} (exit {code})")
                    }
                    None => format!("[tool:shell:{status}] {command}"),
                };
                lines.push(RenderLine::tool_call(summary));

                ParseResult {
                    events,
                    lines,
                    output_buffer_text: None,
                    tool_name: None,
                }
            }
        }
    }

    fn convert_mcp_tool_call(
        &mut self,
        item_id: &str,
        lifecycle: ItemLifecycle,
        server: &str,
        tool: &str,
        status: &CodexToolCallStatus,
        error: Option<CodexErrorItem>,
    ) -> ParseResult {
        let mut events = Vec::new();
        let mut lines = Vec::new();

        match lifecycle {
            ItemLifecycle::Started => {
                events.push(AgentEvent::ToolCallBegin {
                    call_id: item_id.to_string(),
                    tool: format!("mcp:{tool}"),
                    detail: Some(format!("{server}/{tool}")),
                    source: ToolSource::Mcp,
                });
                lines.push(RenderLine::tool_call(format!(
                    "[tool:mcp] {server}/{tool}"
                )));

                ParseResult {
                    events,
                    lines,
                    output_buffer_text: None,
                    tool_name: Some(tool.to_string()),
                }
            }
            ItemLifecycle::Updated => ParseResult {
                events: vec![],
                lines: vec![],
                output_buffer_text: None,
                tool_name: None,
            },
            ItemLifecycle::Completed => {
                let tool_status = match status {
                    CodexToolCallStatus::Completed => ToolStatus::Completed,
                    CodexToolCallStatus::Failed => ToolStatus::Failed,
                    CodexToolCallStatus::InProgress => ToolStatus::Completed,
                };

                events.push(AgentEvent::ToolCallEnd {
                    call_id: item_id.to_string(),
                    tool: format!("mcp:{tool}"),
                    status: tool_status,
                    duration_ms: None,
                    exit_code: None,
                });

                lines.push(RenderLine::tool_call(format!(
                    "[tool:mcp:{status}] {server}/{tool}"
                )));
                if let Some(err) = &error {
                    lines.push(RenderLine::error(format!(
                        "[tool:mcp:{tool}] {}",
                        err.message
                    )));
                }

                ParseResult {
                    events,
                    lines,
                    output_buffer_text: None,
                    tool_name: None,
                }
            }
        }
    }

    fn convert_collab_tool_call(
        &mut self,
        item_id: &str,
        lifecycle: ItemLifecycle,
        tool: &CodexCollabTool,
        status: &CodexToolCallStatus,
    ) -> ParseResult {
        let tool_name = tool.to_string();
        let mut events = Vec::new();
        let mut lines = Vec::new();

        match lifecycle {
            ItemLifecycle::Started => {
                if matches!(tool, CodexCollabTool::SpawnAgent) {
                    events.push(AgentEvent::SubagentSpawned {
                        agent_id: item_id.to_string(),
                        name: None,
                    });
                }
                events.push(AgentEvent::ToolCallBegin {
                    call_id: item_id.to_string(),
                    tool: format!("collab:{tool_name}"),
                    detail: Some(tool_name.clone()),
                    source: ToolSource::Agent,
                });
                lines.push(RenderLine::tool_call(format!(
                    "[tool:collab] {tool_name}"
                )));
                ParseResult {
                    events,
                    lines,
                    output_buffer_text: None,
                    tool_name: Some(tool_name),
                }
            }
            ItemLifecycle::Updated => ParseResult {
                events: vec![],
                lines: vec![],
                output_buffer_text: None,
                tool_name: None,
            },
            ItemLifecycle::Completed => {
                let tool_status = match status {
                    CodexToolCallStatus::Completed => ToolStatus::Completed,
                    CodexToolCallStatus::Failed => ToolStatus::Failed,
                    CodexToolCallStatus::InProgress => ToolStatus::Completed,
                };

                if matches!(tool, CodexCollabTool::CloseAgent) {
                    events.push(AgentEvent::SubagentComplete {
                        agent_id: item_id.to_string(),
                    });
                }

                events.push(AgentEvent::ToolCallEnd {
                    call_id: item_id.to_string(),
                    tool: format!("collab:{tool_name}"),
                    status: tool_status,
                    duration_ms: None,
                    exit_code: None,
                });

                lines.push(RenderLine::tool_call(format!(
                    "[tool:collab:{status}] {tool_name}"
                )));
                ParseResult {
                    events,
                    lines,
                    output_buffer_text: None,
                    tool_name: None,
                }
            }
        }
    }

    fn convert_web_search(
        &mut self,
        item_id: &str,
        lifecycle: ItemLifecycle,
        query: &str,
    ) -> ParseResult {
        let mut events = Vec::new();
        let mut lines = Vec::new();

        match lifecycle {
            ItemLifecycle::Started => {
                events.push(AgentEvent::ToolCallBegin {
                    call_id: item_id.to_string(),
                    tool: "web_search".to_string(),
                    detail: Some(query.to_string()),
                    source: ToolSource::Agent,
                });
                lines.push(RenderLine::tool_call(format!(
                    "[tool:web_search] {query}"
                )));
                ParseResult {
                    events,
                    lines,
                    output_buffer_text: None,
                    tool_name: Some("web_search".to_string()),
                }
            }
            ItemLifecycle::Updated => ParseResult {
                events: vec![],
                lines: vec![],
                output_buffer_text: None,
                tool_name: None,
            },
            ItemLifecycle::Completed => {
                events.push(AgentEvent::ToolCallEnd {
                    call_id: item_id.to_string(),
                    tool: "web_search".to_string(),
                    status: ToolStatus::Completed,
                    duration_ms: None,
                    exit_code: None,
                });
                lines.push(RenderLine::tool_call(format!(
                    "[tool:web_search:completed] {query}"
                )));
                ParseResult {
                    events,
                    lines,
                    output_buffer_text: None,
                    tool_name: None,
                }
            }
        }
    }

    fn convert_file_change(
        &mut self,
        item_id: &str,
        lifecycle: ItemLifecycle,
        changes: &[CodexFileUpdateChange],
        status: &CodexPatchApplyStatus,
    ) -> ParseResult {
        let mut events = Vec::new();
        let mut lines = Vec::new();

        if matches!(lifecycle, ItemLifecycle::Started) {
            events.push(AgentEvent::ToolCallBegin {
                call_id: item_id.to_string(),
                tool: "file_change".to_string(),
                detail: Some(format!("{} file(s)", changes.len())),
                source: ToolSource::Agent,
            });
        }

        if matches!(lifecycle, ItemLifecycle::Completed) {
            let tool_status = match status {
                CodexPatchApplyStatus::Failed => ToolStatus::Failed,
                _ => ToolStatus::Completed,
            };
            events.push(AgentEvent::ToolCallEnd {
                call_id: item_id.to_string(),
                tool: "file_change".to_string(),
                status: tool_status,
                duration_ms: None,
                exit_code: None,
            });

            let change_summary = changes
                .iter()
                .map(|c| format!("{} {}", c.kind, c.path))
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(RenderLine::tool_call(format!(
                "[tool:file_change:{status}] {change_summary}"
            )));
        }

        ParseResult {
            events,
            lines,
            output_buffer_text: None,
            tool_name: if matches!(lifecycle, ItemLifecycle::Started | ItemLifecycle::Completed) {
                Some("file_change".to_string())
            } else {
                None
            },
        }
    }

    fn convert_todo_list(
        &mut self,
        item_id: &str,
        lifecycle: ItemLifecycle,
        items: &[CodexTodoItem],
    ) -> ParseResult {
        let snapshot = render_todo_list(items);
        let changed = match lifecycle {
            ItemLifecycle::Started => {
                self.todo_snapshots
                    .insert(item_id.to_string(), snapshot.clone());
                true
            }
            ItemLifecycle::Updated => {
                let is_new = self.todo_snapshots.get(item_id) != Some(&snapshot);
                self.todo_snapshots
                    .insert(item_id.to_string(), snapshot.clone());
                is_new
            }
            ItemLifecycle::Completed => {
                self.todo_snapshots.remove(item_id);
                true
            }
        };

        let events = vec![AgentEvent::PlanUpdate {
            plan: snapshot.clone(),
        }];

        let lines = if changed {
            snapshot.lines().map(|l| RenderLine::todo(l)).collect()
        } else {
            vec![]
        };

        ParseResult {
            events,
            lines,
            output_buffer_text: None,
            tool_name: None,
        }
    }

    fn take_output_delta(&mut self, item_id: &str, aggregated_output: &str) -> Option<String> {
        let previous_len = self
            .command_output_lengths
            .get(item_id)
            .copied()
            .unwrap_or(0);
        let safe_previous_len = previous_len.min(aggregated_output.len());
        let delta = aggregated_output
            .get(safe_previous_len..)
            .unwrap_or_default();
        self.command_output_lengths
            .insert(item_id.to_string(), aggregated_output.len());
        if delta.is_empty() {
            None
        } else {
            Some(delta.to_string())
        }
    }
}

/// Detects Codex's native spinner/progress lines (which we suppress).
pub fn is_transient_codex_progress_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || !trimmed.contains("to interrupt") {
        return false;
    }
    let has_spinner_prefix = ['\u{25d0}', '\u{25d3}', '\u{25d1}', '\u{25d2}', '\u{2022}']
        .iter()
        .any(|prefix| trimmed.starts_with(*prefix));
    let has_elapsed_segment = trimmed.contains('(')
        && trimmed.contains(')')
        && (trimmed.contains("ms") || trimmed.contains('s'));
    has_spinner_prefix || has_elapsed_segment
}

fn render_todo_list(items: &[CodexTodoItem]) -> String {
    items
        .iter()
        .map(|item| {
            let marker = if item.completed { "[x]" } else { "[ ]" };
            format!("{marker} {}", item.text)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_thread_started() {
        let mut parser = CodexEventParser::default();
        let result = parser
            .parse_line(r#"{"type":"thread.started","thread_id":"thread-123"}"#)
            .unwrap();

        assert_eq!(result.events.len(), 1);
        assert!(matches!(
            &result.events[0],
            AgentEvent::SessionStarted { session_id: Some(id) } if id == "thread-123"
        ));
        assert_eq!(result.lines.len(), 1);
    }

    #[test]
    fn parse_turn_completed_with_tokens() {
        let mut parser = CodexEventParser::default();
        let result = parser
            .parse_line(
                r#"{"type":"turn.completed","usage":{"input_tokens":1200,"cached_input_tokens":300,"output_tokens":99}}"#,
            )
            .unwrap();

        assert!(result.events.iter().any(|e| matches!(
            e,
            AgentEvent::TokenUpdate {
                input: Some(1200),
                cached: Some(300),
                output: Some(99),
            }
        )));
        assert!(result
            .events
            .iter()
            .any(|e| matches!(e, AgentEvent::TurnComplete)));
    }

    #[test]
    fn parse_command_execution_begin_end() {
        let mut parser = CodexEventParser::default();

        let started = parser
            .parse_line(
                r#"{"type":"item.started","item":{"id":"c1","type":"command_execution","command":"ls","aggregated_output":"","status":"in_progress","exit_code":null}}"#,
            )
            .unwrap();
        assert!(started.events.iter().any(|e| matches!(
            e,
            AgentEvent::ToolCallBegin { tool, source: ToolSource::Agent, .. } if tool == "shell"
        )));
        assert_eq!(started.tool_name, Some("shell".to_string()));

        let completed = parser
            .parse_line(
                r#"{"type":"item.completed","item":{"id":"c1","type":"command_execution","command":"ls","aggregated_output":"file1\n","status":"completed","exit_code":0}}"#,
            )
            .unwrap();
        assert!(completed.events.iter().any(|e| matches!(
            e,
            AgentEvent::ToolCallEnd { tool, status: ToolStatus::Completed, exit_code: Some(0), .. } if tool == "shell"
        )));
    }

    #[test]
    fn parse_agent_message_completed() {
        let mut parser = CodexEventParser::default();
        let result = parser
            .parse_line(
                r#"{"type":"item.completed","item":{"id":"m1","type":"agent_message","text":"done"}}"#,
            )
            .unwrap();

        assert_eq!(result.output_buffer_text, Some("done".to_string()));
        assert!(result.events.iter().any(|e| matches!(
            e,
            AgentEvent::TextComplete { text, role: Role::Assistant } if text == "done"
        )));
    }

    #[test]
    fn parse_mcp_tool_call() {
        let mut parser = CodexEventParser::default();
        let started = parser
            .parse_line(
                r#"{"type":"item.started","item":{"id":"m1","type":"mcp_tool_call","server":"fs","tool":"read","status":"in_progress","error":null}}"#,
            )
            .unwrap();
        assert!(started.events.iter().any(|e| matches!(
            e,
            AgentEvent::ToolCallBegin { source: ToolSource::Mcp, .. }
        )));

        let completed = parser
            .parse_line(
                r#"{"type":"item.completed","item":{"id":"m1","type":"mcp_tool_call","server":"fs","tool":"read","status":"completed","error":null}}"#,
            )
            .unwrap();
        assert!(completed.events.iter().any(|e| matches!(
            e,
            AgentEvent::ToolCallEnd { status: ToolStatus::Completed, .. }
        )));
    }

    #[test]
    fn parse_todo_list() {
        let mut parser = CodexEventParser::default();
        let result = parser
            .parse_line(
                r#"{"type":"item.started","item":{"id":"t1","type":"todo_list","items":[{"text":"step one","completed":false}]}}"#,
            )
            .unwrap();

        assert!(result
            .events
            .iter()
            .any(|e| matches!(e, AgentEvent::PlanUpdate { .. })));
        assert_eq!(result.lines.len(), 1);
    }

    #[test]
    fn transient_progress_lines() {
        assert!(is_transient_codex_progress_line(
            "\u{25d0} Writing response (17s \u{00b7} ctrl+c to interrupt) \u{00b7} model gpt-5.3-codex"
        ));
        assert!(!is_transient_codex_progress_line(
            "[tool:shell:completed] powershell -Command 'Get-ChildItem'"
        ));
    }
}
