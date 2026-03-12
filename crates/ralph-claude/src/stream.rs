//! Low-level Claude stream-json line parser.
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamUsage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_input_tokens: Option<i64>,
}

#[derive(Debug, Default)]
pub struct ClaudeStreamParser {
    assembled: String,
    pending_line: String,
}

impl ClaudeStreamParser {
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

        // Extract usage from message events
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
                update.full_text = Some(full_text);
            }

            return update;
        }

        // Non-assistant messages: emit lines but don't assemble
        if let Some(text) = extract_full_text(value) {
            update.emitted_lines = split_nonempty_lines(&text);
        }

        update
    }

    fn append_text(&mut self, text: &str) -> Vec<String> {
        if text.is_empty() {
            return Vec::new();
        }
        self.assembled.push_str(text);
        self.pending_line.push_str(text);

        let mut lines = Vec::new();
        while let Some(pos) = self.pending_line.find('\n') {
            let line = self.pending_line[..pos].to_string();
            self.pending_line = self.pending_line[pos + 1..].to_string();
            if !line.is_empty() {
                lines.push(line);
            }
        }

        lines
    }

    fn apply_full_text(&mut self, full_text: &str) -> (Option<String>, Vec<String>) {
        if full_text.starts_with(&self.assembled) {
            let delta = full_text[self.assembled.len()..].to_string();
            let lines = self.append_text(&delta);
            let delta = (!delta.is_empty()).then_some(delta);
            return (delta, lines);
        }

        self.assembled = full_text.to_string();
        let mut parts: Vec<&str> = full_text.split('\n').collect();
        if parts.is_empty() {
            self.pending_line.clear();
            return (None, Vec::new());
        }
        let tail = parts.pop().unwrap_or_default().to_string();
        self.pending_line = tail;
        let lines = parts
            .into_iter()
            .filter(|line| !line.is_empty())
            .map(|line| line.to_string())
            .collect();
        (None, lines)
    }
}

fn extract_text_delta(value: &Value) -> Option<String> {
    if let Some(delta) = value.get("delta") {
        if let Some(text) = delta.get("text").and_then(Value::as_str) {
            return Some(text.to_string());
        }
        if delta
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|ty| ty == "text_delta")
            && let Some(text) = delta.get("text").and_then(Value::as_str) {
                return Some(text.to_string());
            }
    }

    if value
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|ty| ty == "text_delta")
        && let Some(text) = value.get("text").and_then(Value::as_str) {
            return Some(text.to_string());
        }

    None
}

fn extract_full_text(value: &Value) -> Option<String> {
    if let Some(completion) = value.get("completion").and_then(Value::as_str) {
        return Some(completion.to_string());
    }

    if let Some(message) = value.get("message")
        && let Some(text) = collect_text_blocks(message.get("content")) {
            return Some(text);
        }

    collect_text_blocks(value.get("content"))
}

fn extract_usage(value: &Value) -> Option<StreamUsage> {
    let usage = value
        .get("usage")
        .or_else(|| value.get("message").and_then(|m| m.get("usage")))?;

    let input = usage.get("input_tokens").and_then(Value::as_i64)?;
    let output = usage.get("output_tokens").and_then(Value::as_i64)?;
    let cache_read = usage
        .get("cache_read_input_tokens")
        .and_then(Value::as_i64);

    Some(StreamUsage {
        input_tokens: input,
        output_tokens: output,
        cache_read_input_tokens: cache_read,
    })
}

fn split_nonempty_lines(text: &str) -> Vec<String> {
    text.split('\n')
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect()
}

fn extract_role(value: &Value) -> Option<String> {
    if let Some(role) = value.get("role").and_then(Value::as_str) {
        return Some(role.to_string());
    }
    if let Some(message) = value.get("message")
        && let Some(role) = message.get("role").and_then(Value::as_str) {
            return Some(role.to_string());
        }
    None
}

fn is_assistant_role(role: Option<&str>) -> bool {
    match role {
        None => true,
        Some(role) => role.eq_ignore_ascii_case("assistant"),
    }
}

fn collect_text_blocks(value: Option<&Value>) -> Option<String> {
    let value = value?;
    match value {
        Value::Array(items) => {
            let mut parts = Vec::new();
            for item in items {
                if item
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|ty| ty == "text")
                {
                    if let Some(text) = item.get("text").and_then(Value::as_str) {
                        parts.push(text.to_string());
                    }
                } else if let Some(text) = item.get("text").and_then(Value::as_str) {
                    parts.push(text.to_string());
                }
            }
            (!parts.is_empty()).then(|| parts.join(""))
        }
        Value::Object(map) => {
            if map
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|ty| ty == "text")
            {
                map.get("text")
                    .and_then(Value::as_str)
                    .map(|text| text.to_string())
            } else {
                None
            }
        }
        _ => None,
    }
}

fn find_tool_name(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => {
            if map
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|ty| ty == "tool_use")
            {
                if let Some(name) = map.get("name").and_then(Value::as_str) {
                    return Some(name.to_string());
                }
                if let Some(name) = map.get("tool_name").and_then(Value::as_str) {
                    return Some(name.to_string());
                }
            }
            for v in map.values() {
                if let Some(name) = find_tool_name(v) {
                    return Some(name);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                if let Some(name) = find_tool_name(item) {
                    return Some(name);
                }
            }
        }
        _ => {}
    }
    None
}

fn find_tool_id(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => {
            if map
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|ty| ty == "tool_use" || ty == "tool_result")
                && let Some(id) = map
                    .get("id")
                    .or_else(|| map.get("tool_use_id"))
                    .and_then(Value::as_str)
                {
                    return Some(id.to_string());
                }
            for v in map.values() {
                if let Some(id) = find_tool_id(v) {
                    return Some(id);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                if let Some(id) = find_tool_id(item) {
                    return Some(id);
                }
            }
        }
        _ => {}
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text_delta_events() {
        let mut parser = ClaudeStreamParser::default();
        let update = parser
            .process_line(
                r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"Hello\n"}}"#,
            )
            .unwrap();

        assert_eq!(update.text_delta, Some("Hello\n".to_string()));
        assert_eq!(update.emitted_lines, vec!["Hello".to_string()]);
        assert_eq!(parser.assembled_text(), "Hello\n");
    }

    #[test]
    fn parses_full_message_events() {
        let mut parser = ClaudeStreamParser::default();
        let update = parser
            .process_line(
                r#"{"type":"message","message":{"content":[{"type":"text","text":"Done"}]}}"#,
            )
            .unwrap();

        assert_eq!(update.full_text, Some("Done".to_string()));
        assert!(update.emitted_lines.is_empty());
        assert_eq!(parser.flush_pending(), Some("Done".to_string()));
    }

    #[test]
    fn separates_user_messages() {
        let mut parser = ClaudeStreamParser::default();
        let update = parser
            .process_line(
                r#"{"type":"message","message":{"role":"user","content":[{"type":"text","text":"User says hi"}]}}"#,
            )
            .unwrap();

        assert_eq!(update.full_text, None);
        assert_eq!(update.role.as_deref(), Some("user"));
        assert_eq!(update.emitted_lines, vec!["User says hi".to_string()]);
        assert_eq!(parser.assembled_text(), "");
    }

    #[test]
    fn extracts_tool_use() {
        let mut parser = ClaudeStreamParser::default();
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
        let mut parser = ClaudeStreamParser::default();
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
