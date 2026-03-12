use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use ralph_core::options::AgentOptions;
use ralph_core::plugin::{LoopFeedback, RunContext, Runner};
use ralph_core::state::get_last_message_capture_path;

use crate::behavior::{build_opencode_args, opencode_command};
use crate::parser::OpencodeEventParser;

pub struct OpencodeRunner {
    parser: OpencodeEventParser,
    has_status_renderer: bool,
}

impl OpencodeRunner {
    pub fn new(show_status: bool) -> Self {
        Self {
            parser: OpencodeEventParser,
            has_status_renderer: show_status,
        }
    }
}

impl Runner for OpencodeRunner {
    fn command(&self) -> &str {
        opencode_command()
    }

    fn wants_stdin_prompt(&self) -> bool {
        // On Windows, use stdin to pass prompts with newlines
        // because batch files cannot handle multi-line arguments
        cfg!(target_os = "windows")
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
        use crate::prompt::prepend_opencode_file_edit_prompt;
        prepend_opencode_file_edit_prompt(prompt)
    }

    fn build_args(
        &self,
        prompt: &str,
        model: &str,
        options: &AgentOptions,
    ) -> Vec<String> {
        // On Windows, prompt is passed via stdin, not as argument
        let prompt_arg = if cfg!(target_os = "windows") {
            ""
        } else {
            prompt
        };
        build_opencode_args(prompt_arg, model, options)
    }

    fn build_env(
        &self,
        _options: &AgentOptions,
    ) -> HashMap<String, String> {
        HashMap::new()
    }

    fn handle_stdout(&mut self, line: &str, ctx: &mut RunContext) -> Result<()> {
        match self.parser.parse_line(line) {
            Ok(result) => {
                for event in result.events {
                    ctx.emit(event)?;
                }

                for render_line in result.lines {
                    ctx.render(render_line)?;
                }

                if let Some(text) = &result.output_buffer_text {
                    ctx.output.output_buffer.push_str(text);
                    ctx.output.output_buffer.push('\n');
                }

                if let Some(tool) = &result.tool_name {
                    *ctx.output.tools_used.entry(tool.clone()).or_insert(0) += 1;
                }

                if let Some(response) = &result.latest_response {
                    ctx.output.latest_ai_response = Some(response.clone());
                }
            }
            Err(_) => {
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

        // OpenCode outputs everything to stderr, so parse it like stdout
        match self.parser.parse_line(line) {
            Ok(result) => {
                for event in result.events {
                    ctx.emit(event)?;
                }

                for render_line in result.lines {
                    ctx.render(render_line)?;
                }

                if let Some(text) = &result.output_buffer_text {
                    ctx.output.output_buffer.push_str(text);
                    ctx.output.output_buffer.push('\n');
                }

                if let Some(tool) = &result.tool_name {
                    *ctx.output.tools_used.entry(tool.clone()).or_insert(0) += 1;
                }

                if let Some(response) = &result.latest_response {
                    ctx.output.latest_ai_response = Some(response.clone());
                }
            }
            Err(_) => {
                // If parsing fails, emit as stderr
                ctx.sink.emit_stderr(line)?;
            }
        }

        Ok(())
    }

    fn handle_tick(&mut self, _ctx: &mut RunContext) -> Result<()> {
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
