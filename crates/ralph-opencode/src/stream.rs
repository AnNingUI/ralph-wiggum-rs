//! Low-level Opencode stream-json line parser.
//! Handles text delta assembly and role filtering.

use serde_json::Value;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StreamUpdate {
    pub emitted_lines: Vec<String>,
    pub text_delta: Option<String>,
    pub full_text: Option<String>,
    pub tool_name: Option<String>,
    pub tool_id: Option<String>,
    pub role: Option<String>,
    pub usage: Option<StreamUsage>,
    /// Thinking/reasoning delta text.
    pub thinking_delta: Option<String>,
    /// Accumulated thinking lines ready for display.
    pub thinking_lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamUsage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_input_tokens: Option<i64>,
}

#[derive(Debug, Default)]
pub struct OpencodeStreamParser {
    assembled: String,
    pending_line: String,
    pending_thinking: String,
}

impl OpencodeStreamParser {
    pub fn process_line(&mut self, line: &str) -> Result<StreamUpdate, serde_json::Error> {
        let value: Value = serde_json::from_str(line)?;
        Ok(self.process_value(&value))
    }

    pub fn assembled_text(&self) -> &str {
        &self.assembled
    }

    pub fn flush_pending(&mut self) -> Option<String> {
        if self.pending_line.trim().is_empty() {
            self.pending_line.clear();
            return None;
        }
        let pending = self.pending_line.clone();
        self.pending_line.clear();
        Some(pending)
    }

    fn process_value(&mut self, value: &Value) -> StreamUpdate {
        let mut update = StreamUpdate::default();
        let role = extract_role(value);
        let is_assistant = is_assistant_role(role.as_deref());
        update.role = role;

        // Extract usage
        if let Some(usage) = extract_usage(value) {
            update.usage = Some(usage);
        }

        // Extract tool_use info
        if let Some(tool_name) = find_tool_name(value) {
            update.tool_name = Some(tool_name);
        }
        if let Some(tool_id) = find_tool_id(value) {
            update.tool_id = Some(tool_id);
        }

        // Handle thinking/reasoning content blocks
        if let Some(thinking) = extract_thinking_delta(value) {
            let lines = self.append_thinking(&thinking);
            update.thinking_delta = Some(thinking);
            update.thinking_lines = lines;
            return update;
        }

        if let Some(thinking) = extract_full_thinking(value) {
            let lines = self.append_thinking(&thinking);
            update.thinking_delta = Some(thinking);
            update.thinking_lines = lines;
            return update;
        }

        if is_assistant {
            if let Some(delta) = extract_text_delta(value) {
                let lines = self.append_text(&delta);
                update.text_delta = Some(delta);
                update.emitted_lines = lines;
                update.full_text = Some(self.assembled.clone());
                return update;
            }

            if let Some(full_text) = extract_full_text(value) {
                let (delta, lines) = self.apply_full_text(&full_text);
                update.text_delta = delta;
                update.emitted_lines = lines;
                update.full_text = Some(self.assembled.clone());
                return update;
            }
        }

        // Non-assistant messages: emit immediately
        if let Some(full_text) = extract_full_text(value) {
            update.emitted_lines = vec![full_text];
        }

        update
    }

    fn append_text(&mut self, delta: &str) -> Vec<String> {
        self.assembled.push_str(delta);
        self.pending_line.push_str(delta);

        let mut lines = Vec::new();
        while let Some(idx) = self.pending_line.find('\n') {
            let line = self.pending_line[..idx].to_string();
            lines.push(line);
            self.pending_line = self.pending_line[idx + 1..].to_string();
        }
        lines
    }

    fn apply_full_text(&mut self, full_text: &str) -> (Option<String>, Vec<String>) {
        let delta = if self.assembled.is_empty() {
            Some(full_text.to_string())
        } else {
            None
        };
        self.assembled = full_text.to_string();
        self.pending_line.clear();

        let lines = full_text.lines().map(|s| s.to_string()).collect();
        (delta, lines)
    }

    fn append_thinking(&mut self, text: &str) -> Vec<String> {
        if text.is_empty() {
            return Vec::new();
        }
        self.pending_thinking.push_str(text);
        let mut lines = Vec::new();
        while let Some(pos) = self.pending_thinking.find('\n') {
            let line = self.pending_thinking[..pos].to_string();
            self.pending_thinking = self.pending_thinking[pos + 1..].to_string();
            if !line.is_empty() {
                lines.push(line);
            }
        }
        lines
    }
}

fn extract_thinking_delta(value: &Value) -> Option<String> {
    let delta = value.get("delta")?;
    if delta
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|ty| ty == "thinking_delta")
    {
        return delta
            .get("thinking")
            .and_then(Value::as_str)
            .map(String::from);
    }
    None
}

fn extract_full_thinking(value: &Value) -> Option<String> {
    // content_block_start with type "thinking"
    if let Some(cb) = value.get("content_block") {
        if cb
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|ty| ty == "thinking")
        {
            if let Some(text) = cb.get("thinking").and_then(Value::as_str) {
                if !text.is_empty() {
                    return Some(text.to_string());
                }
            }
            return None;
        }
    }

    // message.content[] blocks of type "thinking"
    if let Some(content) = value
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
    {
        let mut parts = Vec::new();
        for item in content {
            if item
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|ty| ty == "thinking")
            {
                if let Some(text) = item.get("thinking").and_then(Value::as_str) {
                    parts.push(text.to_string());
                }
            }
        }
        if !parts.is_empty() {
            return Some(parts.join(""));
        }
    }

    None
}

fn extract_role(value: &Value) -> Option<String> {
    value
        .get("message")
        .and_then(|m| m.get("role"))
        .and_then(|r| r.as_str())
        .map(|s| s.to_string())
}

fn is_assistant_role(role: Option<&str>) -> bool {
    match role {
        None => true,
        Some(role) => role.eq_ignore_ascii_case("assistant"),
    }
}

fn extract_text_delta(value: &Value) -> Option<String> {
    value
        .get("delta")
        .and_then(|d| d.get("text"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
}

fn extract_full_text(value: &Value) -> Option<String> {
    // From message.content[].text
    if let Some(content) = value.get("message").and_then(|m| m.get("content"))
        && let Some(arr) = content.as_array()
    {
        for item in arr {
            if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                return Some(text.to_string());
            }
        }
    }

    // From content_block.text
    if let Some(text) = value
        .get("content_block")
        .and_then(|cb| cb.get("text"))
        .and_then(|t| t.as_str())
    {
        return Some(text.to_string());
    }

    None
}

fn find_tool_name(value: &Value) -> Option<String> {
    value
        .get("content_block")
        .and_then(|cb| cb.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string())
}

fn find_tool_id(value: &Value) -> Option<String> {
    value
        .get("content_block")
        .and_then(|cb| cb.get("id"))
        .and_then(|id| id.as_str())
        .map(|s| s.to_string())
}

fn extract_usage(value: &Value) -> Option<StreamUsage> {
    let usage = value.get("usage")?;
    let input_tokens = usage.get("input_tokens")?.as_i64()?;
    let output_tokens = usage.get("output_tokens")?.as_i64()?;
    let cache_read_input_tokens = usage
        .get("cache_read_input_tokens")
        .and_then(|v| v.as_i64());

    Some(StreamUsage {
        input_tokens,
        output_tokens,
        cache_read_input_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembles_text_deltas() {
        let mut parser = OpencodeStreamParser::default();
        let update1 = parser
            .process_line(
                r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"Hello"}}"#,
            )
            .unwrap();
        assert_eq!(update1.text_delta, Some("Hello".to_string()));
        assert_eq!(parser.assembled_text(), "Hello");

        let update2 = parser
            .process_line(
                r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":" world"}}"#,
            )
            .unwrap();
        assert_eq!(update2.text_delta, Some(" world".to_string()));
        assert_eq!(parser.assembled_text(), "Hello world");
    }

    #[test]
    fn emits_complete_lines() {
        let mut parser = OpencodeStreamParser::default();
        let update = parser
            .process_line(r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"Line 1\nLine 2\n"}}"#)
            .unwrap();
        assert_eq!(update.emitted_lines, vec!["Line 1", "Line 2"]);
    }

    #[test]
    fn extracts_tool_use() {
        let mut parser = OpencodeStreamParser::default();
        let update = parser
            .process_line(
                r#"{"type":"content_block_start","content_block":{"type":"tool_use","id":"tu_1","name":"Read"}}"#,
            )
            .unwrap();
        assert_eq!(update.tool_name, Some("Read".to_string()));
        assert_eq!(update.tool_id, Some("tu_1".to_string()));
    }

    #[test]
    fn extracts_usage() {
        let mut parser = OpencodeStreamParser::default();
        let update = parser
            .process_line(
                r#"{"type":"message_delta","usage":{"input_tokens":500,"output_tokens":100}}"#,
            )
            .unwrap();
        let usage = update.usage.unwrap();
        assert_eq!(usage.input_tokens, 500);
        assert_eq!(usage.output_tokens, 100);
    }
}
