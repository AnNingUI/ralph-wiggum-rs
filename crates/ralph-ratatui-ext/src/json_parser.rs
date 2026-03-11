//! Incremental JSON parser for streaming data.
//!
//! Handles partial JSON objects and extracts complete objects as they arrive.
//! Designed for stream-json format where multiple JSON objects may arrive
//! in fragments across multiple reads.

use anyhow::{Result, anyhow};
use serde_json::Value;

/// Result of parsing a chunk of JSON data.
#[derive(Debug, Clone)]
pub enum JsonParseResult {
    /// A complete JSON object was parsed.
    Complete(Value),
    /// Partial data, need more input.
    Incomplete,
    /// Parse error (invalid JSON).
    Error(String),
}

/// Incremental JSON parser that accumulates partial data.
///
/// This parser maintains an internal buffer and attempts to parse
/// complete JSON objects as data arrives. It handles:
/// - Multi-line JSON objects
/// - Multiple JSON objects in a single line
/// - Partial JSON fragments
pub struct IncrementalJsonParser {
    buffer: String,
    brace_depth: i32,
    bracket_depth: i32,
    in_string: bool,
    escape_next: bool,
}

impl IncrementalJsonParser {
    /// Create a new incremental JSON parser.
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            brace_depth: 0,
            bracket_depth: 0,
            in_string: false,
            escape_next: false,
        }
    }

    /// Feed a chunk of data to the parser.
    ///
    /// Returns a vector of complete JSON objects that were parsed.
    /// Incomplete data is buffered for the next call.
    pub fn feed(&mut self, chunk: &str) -> Result<Vec<Value>> {
        let mut results = Vec::new();

        for ch in chunk.chars() {
            self.buffer.push(ch);

            // Track string state
            if self.escape_next {
                self.escape_next = false;
                continue;
            }

            match ch {
                '\\' if self.in_string => {
                    self.escape_next = true;
                }
                '"' => {
                    self.in_string = !self.in_string;
                }
                '{' if !self.in_string => {
                    self.brace_depth += 1;
                }
                '}' if !self.in_string => {
                    self.brace_depth -= 1;
                    if self.brace_depth == 0 && self.bracket_depth == 0 {
                        // Complete object found
                        if let Some(obj) = self.try_parse()? {
                            results.push(obj);
                        }
                    }
                }
                '[' if !self.in_string => {
                    self.bracket_depth += 1;
                }
                ']' if !self.in_string => {
                    self.bracket_depth -= 1;
                    if self.brace_depth == 0 && self.bracket_depth == 0 {
                        // Complete array found
                        if let Some(obj) = self.try_parse()? {
                            results.push(obj);
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(results)
    }

    /// Try to parse the current buffer as JSON.
    fn try_parse(&mut self) -> Result<Option<Value>> {
        let trimmed = self.buffer.trim();
        if trimmed.is_empty() {
            self.buffer.clear();
            return Ok(None);
        }

        match serde_json::from_str::<Value>(trimmed) {
            Ok(value) => {
                self.buffer.clear();
                self.reset_state();
                Ok(Some(value))
            }
            Err(e) => {
                // If it's truly invalid, clear and report error
                if self.brace_depth < 0 || self.bracket_depth < 0 {
                    let err_msg = format!("Invalid JSON: {}", e);
                    self.buffer.clear();
                    self.reset_state();
                    Err(anyhow!(err_msg))
                } else {
                    // Still incomplete, keep buffering
                    Ok(None)
                }
            }
        }
    }

    /// Reset parser state.
    fn reset_state(&mut self) {
        self.brace_depth = 0;
        self.bracket_depth = 0;
        self.in_string = false;
        self.escape_next = false;
    }

    /// Get the current buffer size (for debugging).
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    /// Clear the buffer and reset state.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.reset_state();
    }
}

impl Default for IncrementalJsonParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_complete_object() {
        let mut parser = IncrementalJsonParser::new();
        let results = parser.feed(r#"{"key": "value"}"#).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["key"], "value");
    }

    #[test]
    fn test_partial_then_complete() {
        let mut parser = IncrementalJsonParser::new();

        let results1 = parser.feed(r#"{"key": "#).unwrap();
        assert_eq!(results1.len(), 0);

        let results2 = parser.feed(r#""value"}"#).unwrap();
        assert_eq!(results2.len(), 1);
        assert_eq!(results2[0]["key"], "value");
    }

    #[test]
    fn test_multiple_objects_in_one_feed() {
        let mut parser = IncrementalJsonParser::new();
        let results = parser.feed(r#"{"a":1}{"b":2}{"c":3}"#).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0]["a"], 1);
        assert_eq!(results[1]["b"], 2);
        assert_eq!(results[2]["c"], 3);
    }

    #[test]
    fn test_nested_objects() {
        let mut parser = IncrementalJsonParser::new();
        let results = parser.feed(r#"{"outer": {"inner": "value"}}"#).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["outer"]["inner"], "value");
    }

    #[test]
    fn test_string_with_braces() {
        let mut parser = IncrementalJsonParser::new();
        let results = parser.feed(r#"{"text": "hello {world}"}"#).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["text"], "hello {world}");
    }

    #[test]
    fn test_escaped_quotes() {
        let mut parser = IncrementalJsonParser::new();
        let results = parser.feed(r#"{"text": "say \"hello\""}"#).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["text"], r#"say "hello""#);
    }

    #[test]
    fn test_clear() {
        let mut parser = IncrementalJsonParser::new();
        parser.feed(r#"{"partial"#).unwrap();
        assert!(parser.buffer_len() > 0);

        parser.clear();
        assert_eq!(parser.buffer_len(), 0);
    }
}
