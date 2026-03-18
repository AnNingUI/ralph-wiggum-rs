use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use ralph_core::options::AgentOptions;
use ralph_core::plugin::{LoopFeedback, RunContext, Runner};
use ralph_core::state::get_last_message_capture_path;

use crate::behavior::{build_codex_args, codex_command};
use crate::parser::{CodexEventParser, is_transient_codex_progress_line};
use crate::prompt::prepend_codex_file_edit_prompt;

pub struct CodexRunner {
    parser: CodexEventParser,
    has_status_renderer: bool,
}

impl CodexRunner {
    pub fn new(show_status: bool) -> Self {
        Self {
            parser: CodexEventParser::default(),
            has_status_renderer: show_status,
        }
    }
}

impl Runner for CodexRunner {
    fn command(&self) -> &str {
        codex_command()
    }

    fn wants_stdin_prompt(&self) -> bool {
        true
    }

    fn output_capture_path(&self) -> Option<PathBuf> {
        Some(get_last_message_capture_path())
    }

    fn tick_interval(&self) -> Option<Duration> {
        if self.has_status_renderer {
            Some(Duration::from_millis(120))
        } else {
            None
        }
    }

    fn prepare_prompt(&self, prompt: &str) -> String {
        prepend_codex_file_edit_prompt(prompt)
    }

    fn build_args(&self, prompt: &str, model: &str, options: &AgentOptions) -> Vec<String> {
        build_codex_args(prompt, model, options)
    }

    fn build_env(&self, _options: &AgentOptions) -> HashMap<String, String> {
        HashMap::new()
    }

    fn handle_stdout(&mut self, line: &str, ctx: &mut RunContext) -> Result<()> {
        // Suppress Codex's native transient spinner lines
        if is_transient_codex_progress_line(line) {
            return Ok(());
        }

        match self.parser.parse_line(line) {
            Ok(result) => {
                // Emit structured events
                for event in result.events {
                    ctx.emit(event)?;
                }

                // Emit render lines
                for render_line in result.lines {
                    ctx.render(render_line)?;
                }

                // Track output buffer text
                if let Some(text) = &result.output_buffer_text {
                    ctx.output.output_buffer.push_str(text);
                    ctx.output.output_buffer.push('\n');
                }

                // Track tool usage
                if let Some(tool) = &result.tool_name {
                    *ctx.output.tools_used.entry(tool.clone()).or_insert(0) += 1;
                }
            }
            Err(_) => {
                // Non-JSON line — emit as raw stdout
                ctx.sink.emit_stdout(line)?;
            }
        }

        Ok(())
    }

    fn handle_stderr(&mut self, line: &str, ctx: &mut RunContext) -> Result<()> {
        // Check for MCP startup patterns in stderr
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("mcp: ") {
            if let Some(server) = rest.strip_suffix(" starting") {
                ctx.emit(ralph_core::AgentEvent::McpServerUpdate {
                    server: server.trim().to_string(),
                    status: ralph_core::McpStatus::Starting,
                })?;
                return Ok(());
            }
            if let Some(server) = rest.strip_suffix(" ready") {
                ctx.emit(ralph_core::AgentEvent::McpServerUpdate {
                    server: server.trim().to_string(),
                    status: ralph_core::McpStatus::Ready,
                })?;
                return Ok(());
            }
            if let Some((server, _)) = rest.split_once(" failed:") {
                ctx.emit(ralph_core::AgentEvent::McpServerUpdate {
                    server: server.trim().to_string(),
                    status: ralph_core::McpStatus::Failed,
                })?;
                return Ok(());
            }
        }

        if trimmed.starts_with("mcp startup:") {
            return Ok(());
        }

        // Default: pass through to sink
        ctx.sink.emit_stderr(line)?;
        Ok(())
    }

    fn handle_tick(&mut self, _ctx: &mut RunContext) -> Result<()> {
        // Status rendering is handled by OutputSink looking at ProgressSnapshot
        Ok(())
    }

    fn finish(&mut self, _ctx: &mut RunContext) -> Result<()> {
        Ok(())
    }

    fn loop_feedback(&self, _output: &str) -> LoopFeedback {
        LoopFeedback::default()
    }

    fn resolve_latest_response(
        &self,
        exit_code: i32,
        output_buffer: &str,
        handler_latest: Option<String>,
        captured_latest: Option<String>,
    ) -> Option<String> {
        captured_latest.or_else(|| {
            handler_latest.or_else(|| {
                if exit_code == 0 {
                    let fallback = output_buffer.trim();
                    (!fallback.is_empty()).then(|| fallback.to_string())
                } else {
                    None
                }
            })
        })
    }
}
