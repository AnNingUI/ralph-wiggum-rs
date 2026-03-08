use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexRenderLineKind {
    Assistant,
    Reasoning,
    Tool,
    ToolOutput,
    Status,
    Error,
    Todo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodexRenderLine {
    pub kind: CodexRenderLineKind,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexRenderBatch {
    pub lines: Vec<CodexRenderLine>,
    pub output_buffer_text: Option<String>,
    pub tool_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodexProgressSnapshot {
    pub status_line: String,
    pub phase: String,
    pub status_header: Option<String>,
    pub thread_id: Option<String>,
    pub todo_completed: usize,
    pub todo_total: usize,
    pub tool_calls: usize,
    pub last_tool: Option<String>,
    pub last_detail: Option<String>,
    pub last_error: Option<String>,
    pub input_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CodexUiEvent {
    Batch {
        lines: Vec<CodexRenderLine>,
        output_buffer_text: Option<String>,
        tool_name: Option<String>,
        progress: CodexProgressSnapshot,
    },
    RawStdout {
        text: String,
    },
    Stderr {
        text: String,
    },
    ParseError {
        text: String,
    },
}

impl CodexUiEvent {
    pub fn from_batch(batch: &CodexRenderBatch, progress: CodexProgressSnapshot) -> Self {
        Self::Batch {
            lines: batch.lines.clone(),
            output_buffer_text: batch.output_buffer_text.clone(),
            tool_name: batch.tool_name.clone(),
            progress,
        }
    }

    pub fn raw_stdout(text: impl Into<String>) -> Self {
        Self::RawStdout { text: text.into() }
    }

    pub fn stderr(text: impl Into<String>) -> Self {
        Self::Stderr { text: text.into() }
    }

    pub fn parse_error(text: impl Into<String>) -> Self {
        Self::ParseError { text: text.into() }
    }
}

#[derive(Debug, Default)]
pub struct CodexJsonEventProcessor {
    command_output_lengths: HashMap<String, usize>,
    todo_snapshots: HashMap<String, String>,
    ui_state: CodexUiState,
}

impl CodexJsonEventProcessor {
    pub fn process_line(&mut self, line: &str) -> Result<CodexRenderBatch, serde_json::Error> {
        let event: CodexThreadEvent = serde_json::from_str(line)?;
        self.ui_state.update_from_event(&event);

        Ok(match event {
            CodexThreadEvent::ThreadStarted { thread_id } => CodexRenderBatch {
                lines: vec![status_line(format!("[codex] thread started: {thread_id}"))],
                output_buffer_text: None,
                tool_name: None,
            },
            CodexThreadEvent::TurnStarted {} => CodexRenderBatch {
                lines: vec![status_line("[codex] turn started".to_string())],
                output_buffer_text: None,
                tool_name: None,
            },
            CodexThreadEvent::TurnCompleted { usage } => {
                let usage_text = usage
                    .map(|usage| {
                        format!(
                            "[codex] turn completed · in={} cached={} out={}",
                            usage.input_tokens, usage.cached_input_tokens, usage.output_tokens
                        )
                    })
                    .unwrap_or_else(|| "[codex] turn completed".to_string());
                CodexRenderBatch {
                    lines: vec![status_line(usage_text)],
                    output_buffer_text: None,
                    tool_name: None,
                }
            }
            CodexThreadEvent::TurnFailed { error } => CodexRenderBatch {
                lines: vec![error_line(format!(
                    "[codex] turn failed: {}",
                    error.message
                ))],
                output_buffer_text: None,
                tool_name: None,
            },
            CodexThreadEvent::Error { message } => CodexRenderBatch {
                lines: vec![error_line(format!("[codex] error: {message}"))],
                output_buffer_text: None,
                tool_name: None,
            },
            CodexThreadEvent::ItemStarted { item } => self.render_item_started(item),
            CodexThreadEvent::ItemUpdated { item } => self.render_item_updated(item),
            CodexThreadEvent::ItemCompleted { item } => self.render_item_completed(item),
            CodexThreadEvent::Unknown => CodexRenderBatch {
                lines: Vec::new(),
                output_buffer_text: None,
                tool_name: None,
            },
        })
    }

    pub fn current_progress(&self) -> CodexProgressSnapshot {
        self.ui_state.snapshot()
    }

    fn render_item_started(&mut self, item: CodexThreadItem) -> CodexRenderBatch {
        match item.details {
            CodexThreadItemDetails::CommandExecution {
                command,
                aggregated_output,
                ..
            } => {
                let output_delta = self.take_command_output_delta(&item.id, &aggregated_output);
                let mut lines = vec![tool_line(format!("[tool:shell] {command}"))];
                if let Some(delta) = output_delta {
                    lines.extend(delta.lines().map(|line| tool_output_line(line.to_string())));
                }
                CodexRenderBatch {
                    lines,
                    output_buffer_text: None,
                    tool_name: Some("shell".to_string()),
                }
            }
            CodexThreadItemDetails::McpToolCall { server, tool, .. } => CodexRenderBatch {
                lines: vec![tool_line(format!("[tool:mcp] {server}/{tool}"))],
                output_buffer_text: None,
                tool_name: Some(tool),
            },
            CodexThreadItemDetails::WebSearch { query, .. } => CodexRenderBatch {
                lines: vec![tool_line(format!("[tool:web_search] {query}"))],
                output_buffer_text: None,
                tool_name: Some("web_search".to_string()),
            },
            CodexThreadItemDetails::CollabToolCall { tool, .. } => {
                let tool_name = tool.to_string();
                CodexRenderBatch {
                    lines: vec![tool_line(format!("[tool:collab] {tool_name}"))],
                    output_buffer_text: None,
                    tool_name: Some(tool_name),
                }
            }
            CodexThreadItemDetails::TodoList { items } => {
                let snapshot = render_todo_list(&items);
                self.todo_snapshots.insert(item.id, snapshot.clone());
                CodexRenderBatch {
                    lines: snapshot
                        .lines()
                        .map(|line| todo_line(line.to_string()))
                        .collect(),
                    output_buffer_text: None,
                    tool_name: None,
                }
            }
            _ => CodexRenderBatch {
                lines: Vec::new(),
                output_buffer_text: None,
                tool_name: None,
            },
        }
    }

    fn render_item_updated(&mut self, item: CodexThreadItem) -> CodexRenderBatch {
        match item.details {
            CodexThreadItemDetails::CommandExecution {
                aggregated_output, ..
            } => CodexRenderBatch {
                lines: self
                    .take_command_output_delta(&item.id, &aggregated_output)
                    .map(|delta| {
                        delta
                            .lines()
                            .map(|line| tool_output_line(line.to_string()))
                            .collect()
                    })
                    .unwrap_or_default(),
                output_buffer_text: None,
                tool_name: None,
            },
            CodexThreadItemDetails::TodoList { items } => {
                let snapshot = render_todo_list(&items);
                let changed = self.todo_snapshots.get(&item.id) != Some(&snapshot);
                self.todo_snapshots.insert(item.id, snapshot.clone());
                CodexRenderBatch {
                    lines: if changed {
                        snapshot
                            .lines()
                            .map(|line| todo_line(line.to_string()))
                            .collect()
                    } else {
                        Vec::new()
                    },
                    output_buffer_text: None,
                    tool_name: None,
                }
            }
            _ => CodexRenderBatch {
                lines: Vec::new(),
                output_buffer_text: None,
                tool_name: None,
            },
        }
    }

    fn render_item_completed(&mut self, item: CodexThreadItem) -> CodexRenderBatch {
        match item.details {
            CodexThreadItemDetails::AgentMessage { text } => CodexRenderBatch {
                lines: text
                    .lines()
                    .map(|line| assistant_line(line.to_string()))
                    .collect(),
                output_buffer_text: Some(text),
                tool_name: None,
            },
            CodexThreadItemDetails::Reasoning { text } => CodexRenderBatch {
                lines: text
                    .lines()
                    .map(|line| reasoning_line(line.to_string()))
                    .collect(),
                output_buffer_text: None,
                tool_name: None,
            },
            CodexThreadItemDetails::Error { message } => CodexRenderBatch {
                lines: vec![error_line(format!("[codex:item] {message}"))],
                output_buffer_text: None,
                tool_name: None,
            },
            CodexThreadItemDetails::CommandExecution {
                command,
                aggregated_output,
                exit_code,
                status,
            } => {
                let mut lines = Vec::new();
                if let Some(delta) = self.take_command_output_delta(&item.id, &aggregated_output) {
                    lines.extend(delta.lines().map(|line| tool_output_line(line.to_string())));
                }
                let summary = match exit_code {
                    Some(code) => format!("[tool:shell:{status}] {command} (exit {code})"),
                    None => format!("[tool:shell:{status}] {command}"),
                };
                lines.push(tool_line(summary));
                CodexRenderBatch {
                    lines,
                    output_buffer_text: None,
                    tool_name: None,
                }
            }
            CodexThreadItemDetails::McpToolCall {
                server,
                tool,
                status,
                error,
            } => {
                let mut lines = vec![tool_line(format!("[tool:mcp:{status}] {server}/{tool}"))];
                if let Some(error) = error {
                    lines.push(error_line(format!("[tool:mcp:{tool}] {}", error.message)));
                }
                CodexRenderBatch {
                    lines,
                    output_buffer_text: None,
                    tool_name: None,
                }
            }
            CodexThreadItemDetails::WebSearch { query, .. } => CodexRenderBatch {
                lines: vec![tool_line(format!("[tool:web_search:completed] {query}"))],
                output_buffer_text: None,
                tool_name: None,
            },
            CodexThreadItemDetails::FileChange { changes, status } => {
                let change_summary = changes
                    .into_iter()
                    .map(|change| format!("{} {}", change.kind, change.path))
                    .collect::<Vec<_>>()
                    .join(", ");
                CodexRenderBatch {
                    lines: vec![tool_line(format!(
                        "[tool:file_change:{status}] {change_summary}"
                    ))],
                    output_buffer_text: None,
                    tool_name: Some("file_change".to_string()),
                }
            }
            CodexThreadItemDetails::TodoList { items } => {
                let snapshot = render_todo_list(&items);
                self.todo_snapshots.remove(&item.id);
                CodexRenderBatch {
                    lines: snapshot
                        .lines()
                        .map(|line| todo_line(line.to_string()))
                        .collect(),
                    output_buffer_text: None,
                    tool_name: None,
                }
            }
            CodexThreadItemDetails::CollabToolCall { tool, status } => CodexRenderBatch {
                lines: vec![tool_line(format!("[tool:collab:{status}] {tool}"))],
                output_buffer_text: None,
                tool_name: None,
            },
            CodexThreadItemDetails::Unknown => CodexRenderBatch {
                lines: Vec::new(),
                output_buffer_text: None,
                tool_name: None,
            },
        }
    }

    fn take_command_output_delta(
        &mut self,
        item_id: &str,
        aggregated_output: &str,
    ) -> Option<String> {
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct CodexUiState {
    thread_id: Option<String>,
    phase: CodexUiPhase,
    status_header: Option<String>,
    todo_completed: usize,
    todo_total: usize,
    tool_calls: usize,
    last_tool: Option<String>,
    last_detail: Option<String>,
    last_error: Option<String>,
    last_usage: Option<CodexUsage>,
}

impl CodexUiState {
    fn update_from_event(&mut self, event: &CodexThreadEvent) {
        match event {
            CodexThreadEvent::ThreadStarted { thread_id } => {
                self.thread_id = Some(thread_id.clone());
                self.phase = CodexUiPhase::Thread;
                self.status_header = Some("Booting agent".to_string());
                self.last_error = None;
            }
            CodexThreadEvent::TurnStarted {} => {
                self.phase = self.phase_after_background_work();
                self.status_header = Some("Working".to_string());
                self.last_usage = None;
                self.last_error = None;
            }
            CodexThreadEvent::TurnCompleted { usage } => {
                self.phase = CodexUiPhase::Completed;
                self.status_header = Some("Completed".to_string());
                self.last_usage = usage.clone();
                self.last_detail = usage.as_ref().map(|usage| {
                    format!(
                        "in={} cached={} out={}",
                        usage.input_tokens, usage.cached_input_tokens, usage.output_tokens
                    )
                });
                self.last_error = None;
            }
            CodexThreadEvent::TurnFailed { error } => {
                self.phase = CodexUiPhase::Failed;
                self.status_header = Some("Failed".to_string());
                self.last_error = Some(preview_text(&error.message, 48));
            }
            CodexThreadEvent::Error { message } => {
                self.phase = CodexUiPhase::Failed;
                self.status_header = Some("Failed".to_string());
                self.last_error = Some(preview_text(message, 48));
            }
            CodexThreadEvent::ItemStarted { item } => {
                self.update_from_item(item, CodexItemLifecycle::Started)
            }
            CodexThreadEvent::ItemUpdated { item } => {
                self.update_from_item(item, CodexItemLifecycle::Updated)
            }
            CodexThreadEvent::ItemCompleted { item } => {
                self.update_from_item(item, CodexItemLifecycle::Completed)
            }
            CodexThreadEvent::Unknown => {}
        }
    }

    fn update_from_item(&mut self, item: &CodexThreadItem, lifecycle: CodexItemLifecycle) {
        match &item.details {
            CodexThreadItemDetails::AgentMessage { text } => {
                if matches!(lifecycle, CodexItemLifecycle::Completed) {
                    self.phase = CodexUiPhase::Responding;
                    self.status_header = Some("Writing response".to_string());
                    self.last_detail = Some(preview_text(text, 48));
                    self.last_error = None;
                }
            }
            CodexThreadItemDetails::Reasoning { text } => {
                self.phase = CodexUiPhase::Reasoning;
                self.status_header =
                    extract_reasoning_header(text).or_else(|| Some("Analyzing".to_string()));
                self.last_detail = Some(preview_text(text, 48));
            }
            CodexThreadItemDetails::CommandExecution {
                command, status, ..
            } => match lifecycle {
                CodexItemLifecycle::Started => {
                    self.record_tool_start("shell", Some(command));
                }
                CodexItemLifecycle::Updated => {
                    self.phase = CodexUiPhase::Tool;
                    self.last_tool = Some("shell".to_string());
                    self.last_detail = Some(preview_text(command, 48));
                }
                CodexItemLifecycle::Completed => {
                    self.last_tool = Some("shell".to_string());
                    self.last_detail = Some(preview_text(command, 48));
                    if matches!(
                        status,
                        CodexCommandExecutionStatus::Failed | CodexCommandExecutionStatus::Declined
                    ) {
                        self.phase = CodexUiPhase::Failed;
                        self.last_error = Some(format!("shell {status}"));
                    } else {
                        self.phase = self.phase_after_background_work();
                        self.last_error = None;
                    }
                }
            },
            CodexThreadItemDetails::McpToolCall {
                server,
                tool,
                status,
                error,
            } => match lifecycle {
                CodexItemLifecycle::Started => {
                    self.record_tool_start(
                        &format!("mcp:{tool}"),
                        Some(&format!("{server}/{tool}")),
                    );
                }
                CodexItemLifecycle::Updated => {
                    self.phase = CodexUiPhase::Tool;
                    self.last_tool = Some(format!("mcp:{tool}"));
                    self.last_detail = Some(preview_text(&format!("{server}/{tool}"), 48));
                }
                CodexItemLifecycle::Completed => {
                    self.last_tool = Some(format!("mcp:{tool}"));
                    self.last_detail = Some(preview_text(&format!("{server}/{tool}"), 48));
                    if let Some(error) = error {
                        self.phase = CodexUiPhase::Failed;
                        self.last_error = Some(preview_text(&error.message, 48));
                    } else if matches!(status, CodexToolCallStatus::Failed) {
                        self.phase = CodexUiPhase::Failed;
                        self.last_error = Some(format!("mcp {tool} failed"));
                    } else {
                        self.phase = self.phase_after_background_work();
                        self.last_error = None;
                    }
                }
            },
            CodexThreadItemDetails::CollabToolCall { tool, status } => match lifecycle {
                CodexItemLifecycle::Started => {
                    self.record_tool_start(&format!("collab:{tool}"), Some(&tool.to_string()));
                }
                CodexItemLifecycle::Updated => {
                    self.phase = CodexUiPhase::Tool;
                    self.last_tool = Some(format!("collab:{tool}"));
                    self.last_detail = Some(tool.to_string());
                }
                CodexItemLifecycle::Completed => {
                    self.last_tool = Some(format!("collab:{tool}"));
                    self.last_detail = Some(tool.to_string());
                    if matches!(status, CodexToolCallStatus::Failed) {
                        self.phase = CodexUiPhase::Failed;
                        self.last_error = Some(format!("collab {tool} failed"));
                    } else {
                        self.phase = self.phase_after_background_work();
                        self.last_error = None;
                    }
                }
            },
            CodexThreadItemDetails::WebSearch { query, .. } => match lifecycle {
                CodexItemLifecycle::Started => {
                    self.record_tool_start("web_search", Some(query));
                }
                CodexItemLifecycle::Updated => {
                    self.phase = CodexUiPhase::Tool;
                    self.last_tool = Some("web_search".to_string());
                    self.last_detail = Some(preview_text(query, 48));
                }
                CodexItemLifecycle::Completed => {
                    self.phase = self.phase_after_background_work();
                    self.last_tool = Some("web_search".to_string());
                    self.last_detail = Some(preview_text(query, 48));
                    self.last_error = None;
                }
            },
            CodexThreadItemDetails::TodoList { items } => {
                self.update_todos(items);
                self.phase = CodexUiPhase::Planning;
                self.status_header = Some("Planning".to_string());
                self.last_detail = Some(format!("{} step(s)", items.len()));
            }
            CodexThreadItemDetails::FileChange { changes, status } => {
                if matches!(lifecycle, CodexItemLifecycle::Completed) {
                    self.tool_calls += 1;
                }
                self.status_header
                    .get_or_insert_with(|| "Patching code".to_string());
                self.last_tool = Some("file_change".to_string());
                self.last_detail = Some(format!("{} file(s)", changes.len()));
                if matches!(status, CodexPatchApplyStatus::Failed) {
                    self.phase = CodexUiPhase::Failed;
                    self.status_header = Some("Patch failed".to_string());
                    self.last_error = Some("patch apply failed".to_string());
                } else {
                    self.phase = self.phase_after_background_work();
                    self.last_error = None;
                }
            }
            CodexThreadItemDetails::Error { message } => {
                self.phase = CodexUiPhase::Failed;
                self.status_header = Some("Failed".to_string());
                self.last_error = Some(preview_text(message, 48));
            }
            CodexThreadItemDetails::Unknown => {}
        }
    }

    fn record_tool_start(&mut self, tool_name: &str, detail: Option<&str>) {
        self.phase = CodexUiPhase::Tool;
        self.tool_calls += 1;
        self.last_tool = Some(tool_name.to_string());
        self.last_detail = detail.map(|detail| preview_text(detail, 48));
        self.last_error = None;
    }

    fn update_todos(&mut self, items: &[CodexTodoItem]) {
        self.todo_total = items.len();
        self.todo_completed = items.iter().filter(|item| item.completed).count();
    }

    fn phase_after_background_work(&self) -> CodexUiPhase {
        if self.todo_total > 0 && self.todo_completed < self.todo_total {
            CodexUiPhase::Planning
        } else {
            CodexUiPhase::Thinking
        }
    }

    fn snapshot(&self) -> CodexProgressSnapshot {
        let mut segments = vec![format!("⌘ Codex {}", self.phase.label())];

        if let Some(thread_id) = &self.thread_id {
            segments.push(format!("thr {}", short_thread_id(thread_id)));
        }

        if self.todo_total > 0 {
            let bar = build_progress_bar(self.todo_completed, self.todo_total, 10);
            segments.push(format!(
                "todo {} {}/{}",
                bar, self.todo_completed, self.todo_total
            ));
        }

        segments.push(format!("tools {}", self.tool_calls));

        if let Some(last_tool) = &self.last_tool {
            segments.push(format!("last {}", last_tool));
        }

        if let Some(last_error) = &self.last_error {
            segments.push(format!("err {}", preview_text(last_error, 24)));
        }

        CodexProgressSnapshot {
            status_line: segments.join(" | "),
            phase: self.phase.label().to_string(),
            status_header: self.status_header.clone(),
            thread_id: self.thread_id.clone(),
            todo_completed: self.todo_completed,
            todo_total: self.todo_total,
            tool_calls: self.tool_calls,
            last_tool: self.last_tool.clone(),
            last_detail: self.last_detail.clone(),
            last_error: self.last_error.clone(),
            input_tokens: self.last_usage.as_ref().map(|usage| usage.input_tokens),
            cached_input_tokens: self
                .last_usage
                .as_ref()
                .map(|usage| usage.cached_input_tokens),
            output_tokens: self.last_usage.as_ref().map(|usage| usage.output_tokens),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum CodexUiPhase {
    #[default]
    Idle,
    Thread,
    Thinking,
    Planning,
    Reasoning,
    Tool,
    Responding,
    Completed,
    Failed,
}

impl CodexUiPhase {
    fn label(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Thread => "thread",
            Self::Thinking => "thinking",
            Self::Planning => "planning",
            Self::Reasoning => "reasoning",
            Self::Tool => "tool",
            Self::Responding => "responding",
            Self::Completed => "done",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodexItemLifecycle {
    Started,
    Updated,
    Completed,
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

fn build_progress_bar(completed: usize, total: usize, width: usize) -> String {
    if total == 0 || width == 0 {
        return "".to_string();
    }

    let filled = (completed.saturating_mul(width) / total).min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

fn short_thread_id(thread_id: &str) -> String {
    let tail_len = 8;
    if thread_id.chars().count() <= tail_len {
        thread_id.to_string()
    } else {
        thread_id
            .chars()
            .rev()
            .take(tail_len)
            .collect::<String>()
            .chars()
            .rev()
            .collect()
    }
}

fn preview_text(text: &str, max_chars: usize) -> String {
    let single_line = text.replace(['\r', '\n'], " ");
    let trimmed = single_line.trim();
    if trimmed.chars().count() <= max_chars {
        trimmed.to_string()
    } else {
        let mut preview = trimmed.chars().take(max_chars).collect::<String>();
        preview.push('…');
        preview
    }
}

fn extract_reasoning_header(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(inner) = trimmed
            .strip_prefix("**")
            .and_then(|line| line.strip_suffix("**"))
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

fn assistant_line(text: String) -> CodexRenderLine {
    CodexRenderLine {
        kind: CodexRenderLineKind::Assistant,
        text,
    }
}

fn reasoning_line(text: String) -> CodexRenderLine {
    CodexRenderLine {
        kind: CodexRenderLineKind::Reasoning,
        text,
    }
}

fn tool_line(text: String) -> CodexRenderLine {
    CodexRenderLine {
        kind: CodexRenderLineKind::Tool,
        text,
    }
}

fn tool_output_line(text: String) -> CodexRenderLine {
    CodexRenderLine {
        kind: CodexRenderLineKind::ToolOutput,
        text,
    }
}

fn status_line(text: String) -> CodexRenderLine {
    CodexRenderLine {
        kind: CodexRenderLineKind::Status,
        text,
    }
}

fn error_line(text: String) -> CodexRenderLine {
    CodexRenderLine {
        kind: CodexRenderLineKind::Error,
        text,
    }
}

fn todo_line(text: String) -> CodexRenderLine {
    CodexRenderLine {
        kind: CodexRenderLineKind::Todo,
        text,
    }
}

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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_thread_started_event() {
        let mut processor = CodexJsonEventProcessor::default();
        let batch = processor
            .process_line(r#"{"type":"thread.started","thread_id":"thread-123"}"#)
            .unwrap();
        let progress = processor.current_progress();

        assert_eq!(
            batch,
            CodexRenderBatch {
                lines: vec![CodexRenderLine {
                    kind: CodexRenderLineKind::Status,
                    text: "[codex] thread started: thread-123".to_string(),
                }],
                output_buffer_text: None,
                tool_name: None,
            }
        );
        assert_eq!(progress.phase, "thread");
        assert_eq!(progress.thread_id, Some("thread-123".to_string()));
        assert!(
            progress.status_line.contains("thread-123")
                || progress.status_line.contains("read-123")
        );
    }

    #[test]
    fn parses_agent_message_completion() {
        let mut processor = CodexJsonEventProcessor::default();
        let batch = processor
            .process_line(
                r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"done"}}"#,
            )
            .unwrap();
        let progress = processor.current_progress();

        assert_eq!(batch.output_buffer_text, Some("done".to_string()));
        assert_eq!(
            batch.lines,
            vec![CodexRenderLine {
                kind: CodexRenderLineKind::Assistant,
                text: "done".to_string(),
            }]
        );
        assert_eq!(progress.phase, "responding");
        assert_eq!(progress.last_detail, Some("done".to_string()));
    }

    #[test]
    fn parses_command_execution_start_and_output_delta() {
        let mut processor = CodexJsonEventProcessor::default();
        let started = processor
            .process_line(
                r#"{"type":"item.started","item":{"id":"item_2","type":"command_execution","command":"ls","aggregated_output":"","status":"in_progress","exit_code":null}}"#,
            )
            .unwrap();
        let updated = processor
            .process_line(
                r#"{"type":"item.updated","item":{"id":"item_2","type":"command_execution","command":"ls","aggregated_output":"file1\nfile2\n","status":"in_progress","exit_code":null}}"#,
            )
            .unwrap();
        let progress = processor.current_progress();

        assert_eq!(started.tool_name, Some("shell".to_string()));
        assert_eq!(started.lines[0].text, "[tool:shell] ls");
        assert_eq!(
            updated.lines,
            vec![
                CodexRenderLine {
                    kind: CodexRenderLineKind::ToolOutput,
                    text: "file1".to_string(),
                },
                CodexRenderLine {
                    kind: CodexRenderLineKind::ToolOutput,
                    text: "file2".to_string(),
                },
            ]
        );
        assert_eq!(progress.phase, "tool");
        assert_eq!(progress.tool_calls, 1);
        assert_eq!(progress.last_tool, Some("shell".to_string()));
    }

    #[test]
    fn builds_batch_ui_event() {
        let batch = CodexRenderBatch {
            lines: vec![CodexRenderLine {
                kind: CodexRenderLineKind::Assistant,
                text: "hello".to_string(),
            }],
            output_buffer_text: Some("hello".to_string()),
            tool_name: Some("shell".to_string()),
        };
        let progress = CodexProgressSnapshot {
            status_line: "⌘ Codex tool | tools 1".to_string(),
            phase: "tool".to_string(),
            status_header: Some("Patching code".to_string()),
            thread_id: Some("thread-1".to_string()),
            todo_completed: 0,
            todo_total: 0,
            tool_calls: 1,
            last_tool: Some("shell".to_string()),
            last_detail: Some("ls".to_string()),
            last_error: None,
            input_tokens: None,
            cached_input_tokens: None,
            output_tokens: None,
        };

        let event = CodexUiEvent::from_batch(&batch, progress.clone());

        assert_eq!(
            event,
            CodexUiEvent::Batch {
                lines: batch.lines,
                output_buffer_text: Some("hello".to_string()),
                tool_name: Some("shell".to_string()),
                progress,
            }
        );
    }

    #[test]
    fn parses_todo_list_changes_once_per_snapshot() {
        let mut processor = CodexJsonEventProcessor::default();
        let first = processor
            .process_line(
                r#"{"type":"item.started","item":{"id":"item_3","type":"todo_list","items":[{"text":"step one","completed":false}]}}"#,
            )
            .unwrap();
        let unchanged = processor
            .process_line(
                r#"{"type":"item.updated","item":{"id":"item_3","type":"todo_list","items":[{"text":"step one","completed":false}]}}"#,
            )
            .unwrap();
        let progress = processor.current_progress();

        assert_eq!(first.lines.len(), 1);
        assert!(unchanged.lines.is_empty());
        assert_eq!(progress.phase, "planning");
        assert_eq!(progress.todo_total, 1);
        assert_eq!(progress.todo_completed, 0);
        assert!(progress.status_line.contains("todo"));
    }

    #[test]
    fn captures_usage_on_turn_completed() {
        let mut processor = CodexJsonEventProcessor::default();
        processor
            .process_line(
                r#"{"type":"turn.completed","usage":{"input_tokens":1200,"cached_input_tokens":300,"output_tokens":99}}"#,
            )
            .unwrap();

        let progress = processor.current_progress();

        assert_eq!(progress.phase, "done");
        assert_eq!(progress.input_tokens, Some(1200));
        assert_eq!(progress.cached_input_tokens, Some(300));
        assert_eq!(progress.output_tokens, Some(99));
    }

    #[test]
    fn extract_reasoning_header_prefers_markdown_heading() {
        assert_eq!(
            extract_reasoning_header("**Testing and patching code**\nChecking files..."),
            Some("Testing and patching code".to_string())
        );
    }
}
