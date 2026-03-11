//! Claude stream-json → AgentEvent parser.
//! Converts low-level StreamUpdate into unified AgentEvent + RenderLine.

use ralph_core::{AgentEvent, RenderLine, Role, ToolSource, ToolStatus};

use crate::stream::{ClaudeStreamParser, StreamUpdate};

/// High-level parser that converts Claude stream-json into AgentEvent + RenderLine.
pub struct ClaudeEventParser {
    stream: ClaudeStreamParser,
    replay_user_messages: bool,
    /// Track pending tool_use calls (id → name) for tool_result matching.
    pending_tools: std::collections::HashMap<String, String>,
}

pub struct ParseResult {
    pub events: Vec<AgentEvent>,
    pub lines: Vec<RenderLine>,
    pub output_buffer_text: Option<String>,
    pub latest_response: Option<String>,
}

impl ClaudeEventParser {
    pub fn new(replay_user_messages: bool) -> Self {
        Self {
            stream: ClaudeStreamParser::default(),
            replay_user_messages,
            pending_tools: std::collections::HashMap::new(),
        }
    }

    pub fn parse_line(&mut self, line: &str) -> Result<ParseResult, serde_json::Error> {
        let update = self.stream.process_line(line)?;
        Ok(self.convert(update))
    }

    pub fn flush(&mut self) -> ParseResult {
        let pending = self.stream.flush_pending();
        let assembled = self.stream.assembled_text().trim().to_string();

        let mut lines = Vec::new();
        if let Some(pending) = &pending {
            lines.push(RenderLine::assistant(pending.as_str()));
        }

        ParseResult {
            events: vec![],
            lines,
            output_buffer_text: None,
            latest_response: if assembled.is_empty() {
                None
            } else {
                Some(assembled)
            },
        }
    }

    fn convert(&mut self, update: StreamUpdate) -> ParseResult {
        let mut events = Vec::new();
        let mut lines = Vec::new();
        let mut output_buffer_text = None;
        let mut latest_response = None;

        let is_assistant = update
            .role
            .as_deref()
            .map(|r| r.eq_ignore_ascii_case("assistant"))
            .unwrap_or(true);

        // Token update
        if let Some(usage) = &update.usage {
            events.push(AgentEvent::TokenUpdate {
                input: Some(usage.input_tokens),
                cached: usage.cache_read_input_tokens,
                output: Some(usage.output_tokens),
            });
        }

        // Tool use detection → ToolCallBegin
        if let Some(tool_name) = &update.tool_name {
            let call_id = update
                .tool_id
                .clone()
                .unwrap_or_else(|| format!("claude_tool_{}", self.pending_tools.len()));

            self.pending_tools
                .insert(call_id.clone(), tool_name.clone());

            events.push(AgentEvent::ToolCallBegin {
                call_id: call_id.clone(),
                tool: tool_name.clone(),
                detail: None,
                source: ToolSource::Agent,
            });
            lines.push(RenderLine::tool_call(format!("[tool:{tool_name}]")));
        }

        // Tool result detection → ToolCallEnd
        // Check if this is a tool_result event by looking at the raw JSON structure
        if update.tool_name.is_none() {
            if let Some(tool_id) = &update.tool_id {
                if let Some(tool_name) = self.pending_tools.remove(tool_id) {
                    events.push(AgentEvent::ToolCallEnd {
                        call_id: tool_id.clone(),
                        tool: tool_name.clone(),
                        status: ToolStatus::Completed,
                        duration_ms: None,
                        exit_code: None,
                    });
                    lines.push(RenderLine::tool_output(format!(
                        "[tool:{tool_name}:completed]"
                    )));
                }
            }
        }

        // Text handling
        if is_assistant {
            if let Some(delta) = &update.text_delta {
                events.push(AgentEvent::TextDelta {
                    text: delta.clone(),
                    role: Role::Assistant,
                });
                output_buffer_text = Some(delta.clone());
            }
            if let Some(full_text) = &update.full_text {
                if !full_text.trim().is_empty() {
                    latest_response = Some(full_text.clone());
                }
            }
            for emitted in &update.emitted_lines {
                lines.push(RenderLine::assistant(emitted.as_str()));
            }
        } else {
            // Non-assistant text
            let should_emit = self.replay_user_messages;
            if should_emit {
                for emitted in &update.emitted_lines {
                    lines.push(RenderLine::status(format!(
                        "[{}] {}",
                        update.role.as_deref().unwrap_or("system"),
                        emitted
                    )));
                }
            }
        }

        ParseResult {
            events,
            lines,
            output_buffer_text,
            latest_response,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_text_delta_to_event() {
        let mut parser = ClaudeEventParser::new(false);
        let result = parser
            .parse_line(
                r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"Hello\n"}}"#,
            )
            .unwrap();

        assert!(result.events.iter().any(|e| matches!(
            e,
            AgentEvent::TextDelta { role: Role::Assistant, .. }
        )));
        assert_eq!(result.lines.len(), 1);
        assert_eq!(result.output_buffer_text, Some("Hello\n".to_string()));
    }

    #[test]
    fn converts_tool_use_to_begin_event() {
        let mut parser = ClaudeEventParser::new(false);
        let result = parser
            .parse_line(
                r#"{"type":"content_block_start","content_block":{"type":"tool_use","id":"tu_1","name":"Edit"}}"#,
            )
            .unwrap();

        assert!(result.events.iter().any(|e| matches!(
            e,
            AgentEvent::ToolCallBegin { tool, source: ToolSource::Agent, .. } if tool == "Edit"
        )));
    }

    #[test]
    fn converts_usage_to_token_update() {
        let mut parser = ClaudeEventParser::new(false);
        let result = parser
            .parse_line(
                r#"{"type":"message_delta","usage":{"input_tokens":500,"output_tokens":100}}"#,
            )
            .unwrap();

        assert!(result.events.iter().any(|e| matches!(
            e,
            AgentEvent::TokenUpdate { input: Some(500), output: Some(100), .. }
        )));
    }

    #[test]
    fn skips_user_messages_when_replay_disabled() {
        let mut parser = ClaudeEventParser::new(false);
        let result = parser
            .parse_line(
                r#"{"type":"message","message":{"role":"user","content":[{"type":"text","text":"Hi"}]}}"#,
            )
            .unwrap();

        assert!(result.lines.is_empty());
        assert!(result.output_buffer_text.is_none());
    }

    #[test]
    fn emits_user_messages_when_replay_enabled() {
        let mut parser = ClaudeEventParser::new(true);
        let result = parser
            .parse_line(
                r#"{"type":"message","message":{"role":"user","content":[{"type":"text","text":"Hi"}]}}"#,
            )
            .unwrap();

        assert!(!result.lines.is_empty());
    }

    #[test]
    fn flush_emits_pending_and_latest() {
        let mut parser = ClaudeEventParser::new(false);
        parser
            .parse_line(
                r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"partial"}}"#,
            )
            .unwrap();

        let result = parser.flush();
        assert_eq!(result.lines.len(), 1);
        assert_eq!(result.latest_response, Some("partial".to_string()));
    }
}
