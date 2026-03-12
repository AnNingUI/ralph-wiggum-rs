use anyhow::Result;
use ralph_core::{AgentEvent, RenderKind, RenderLine};
use serde_json::Value;

#[derive(Debug, Default)]
pub struct OpencodeEventParser;

#[derive(Debug)]
pub struct ParseResult {
    pub events: Vec<AgentEvent>,
    pub lines: Vec<RenderLine>,
    pub output_buffer_text: Option<String>,
    pub tool_name: Option<String>,
    pub latest_response: Option<String>,
}

impl OpencodeEventParser {
    pub fn parse_line(&mut self, line: &str) -> Result<ParseResult> {
        let mut result = ParseResult {
            events: Vec::new(),
            lines: Vec::new(),
            output_buffer_text: None,
            tool_name: None,
            latest_response: None,
        };

        // Try to parse as JSON
        if let Ok(json) = serde_json::from_str::<Value>(line) {
            // Only treat objects as structured events
            if json.is_object() {
                self.parse_json_event(&json, &mut result)?;
            } else {
                // Primitive JSON values (numbers, strings, etc.) - treat as plain text
                let text = json.to_string();
                result.lines.push(RenderLine {
                    kind: RenderKind::Assistant,
                    text: text.clone(),
                });
                result.latest_response = Some(text);
            }
        } else {
            // Plain text output - treat as assistant response
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                result.lines.push(RenderLine {
                    kind: RenderKind::Assistant,
                    text: line.to_string(),
                });
                result.latest_response = Some(trimmed.to_string());
            }
        }

        Ok(result)
    }

    fn parse_json_event(&self, json: &Value, result: &mut ParseResult) -> Result<()> {
        // Parse OpenCode's JSON output format
        // Reference: OpenCode uses JSON events for tool calls, messages, and status updates
        if let Some(event_type) = json.get("type").and_then(|v| v.as_str()) {
            match event_type {
                "tool_use" => {
                    if let Some(tool_name) = json.get("tool").and_then(|v| v.as_str()) {
                        result.tool_name = Some(tool_name.to_string());
                        let call_id = json.get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        result.events.push(AgentEvent::ToolCallBegin {
                            call_id,
                            tool: tool_name.to_string(),
                            detail: None,
                            source: ralph_core::ToolSource::Agent,
                        });
                    }
                    result.latest_response = json.get("input").and_then(|v| v.as_str()).map(String::from);
                }
                "tool_result" => {
                    if let Some(output) = json.get("output").and_then(|v| v.as_str()) {
                        result.output_buffer_text = Some(output.to_string());
                    }
                }
                "message" => {
                    if let Some(text) = json.get("text").and_then(|v| v.as_str()) {
                        result.lines.push(RenderLine {
                            kind: RenderKind::Assistant,
                            text: text.to_string(),
                        });
                        result.latest_response = Some(text.to_string());
                    }
                }
                "thinking" => {
                    if let Some(text) = json.get("text").and_then(|v| v.as_str()) {
                        result.lines.push(RenderLine {
                            kind: RenderKind::Assistant,
                            text: format!("[Thinking] {}", text),
                        });
                    }
                }
                "status" => {
                    if let Some(text) = json.get("text").and_then(|v| v.as_str()) {
                        result.lines.push(RenderLine {
                            kind: RenderKind::Status,
                            text: text.to_string(),
                        });
                    }
                }
                "error" => {
                    if let Some(text) = json.get("message").and_then(|v| v.as_str()) {
                        result.lines.push(RenderLine {
                            kind: RenderKind::Assistant,
                            text: format!("[Error] {}", text),
                        });
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }
}
