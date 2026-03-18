use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

use ralph_core::options::AgentOptions;
use ralph_core::plugin::{LoopFeedback, RunContext, Runner};
use ralph_core::types::{ClaudeLoopMode, ClaudeOutputFormat};
use ralph_core::{AgentEvent, McpStatus, RenderLine};

use crate::behavior::{build_claude_args, claude_command};
use crate::loop_state::{self, ClaudeLoopEvent, ClaudeLoopOutcome};
use crate::parser::ClaudeEventParser;

#[derive(Debug, Default)]
struct LoopTracker {
    last_iteration: Option<u32>,
    last_outcome: Option<ClaudeLoopOutcome>,
}

impl LoopTracker {
    fn observe_line(&mut self, line: &str) {
        let Some(event) = loop_state::parse_event(line) else {
            return;
        };
        match event {
            ClaudeLoopEvent::IterationAdvanced(iteration) => {
                self.last_iteration = Some(iteration);
            }
            ClaudeLoopEvent::Outcome(outcome) => {
                self.last_outcome = Some(outcome);
            }
        }
    }
}

pub struct ClaudeRunner {
    event_parser: Option<ClaudeEventParser>,
    loop_mode: ClaudeLoopMode,
    output_format: ClaudeOutputFormat,
    loop_tracker: LoopTracker,
    project_dir: Option<PathBuf>,
}

impl ClaudeRunner {
    pub fn new(
        output_format: ClaudeOutputFormat,
        loop_mode: ClaudeLoopMode,
        replay_user_messages: bool,
    ) -> Self {
        let event_parser = (output_format != ClaudeOutputFormat::Text)
            .then(|| ClaudeEventParser::new(replay_user_messages));
        Self {
            event_parser,
            loop_mode,
            output_format,
            loop_tracker: LoopTracker::default(),
            project_dir: None,
        }
    }

    fn observe_loop_line(&mut self, line: &str) {
        if self.loop_mode == ClaudeLoopMode::RalphPlugin {
            self.loop_tracker.observe_line(line);
        }
    }
}

impl Runner for ClaudeRunner {
    fn command(&self) -> &str {
        claude_command()
    }

    fn wants_stdin_prompt(&self) -> bool {
        false
    }

    fn output_capture_path(&self) -> Option<PathBuf> {
        None
    }

    fn tick_interval(&self) -> Option<std::time::Duration> {
        None
    }

    fn prepare_prompt(&self, prompt: &str) -> String {
        prompt.to_string()
    }

    fn needs_streaming_json(&self) -> bool {
        // Claude Code uses stream-json format, needs incremental parsing
        matches!(self.output_format, ClaudeOutputFormat::StreamJson)
    }

    fn build_args(&self, prompt: &str, model: &str, options: &AgentOptions) -> Vec<String> {
        build_claude_args(prompt, model, options)
    }

    fn build_env(&self, _options: &AgentOptions) -> HashMap<String, String> {
        HashMap::new()
    }

    fn before_spawn(&mut self, _prompt: &str, ctx: &mut RunContext) -> Result<()> {
        self.project_dir = Some(ctx.project_dir.to_path_buf());
        Ok(())
    }

    fn handle_stdout(&mut self, line: &str, ctx: &mut RunContext) -> Result<()> {
        if let Some(parser) = self.event_parser.as_mut() {
            match parser.parse_line(line) {
                Ok(result) => {
                    // Emit structured events
                    for event in result.events {
                        ctx.emit(event)?;
                    }
                    // Emit render lines
                    for render_line in result.lines {
                        // Also check for loop events in assistant lines
                        if matches!(render_line.kind, ralph_core::RenderKind::Assistant) {
                            self.observe_loop_line(&render_line.text);
                        }
                        ctx.render(render_line)?;
                    }
                    // Track output
                    if let Some(delta) = &result.output_buffer_text {
                        ctx.output.output_buffer.push_str(delta);
                    }
                    if let Some(latest) = result.latest_response {
                        ctx.output.latest_ai_response = Some(latest);
                    }
                    return Ok(());
                }
                Err(_) => {
                    // Fall through to plain output
                }
            }
        }

        // Plain text mode or parse error fallback
        ctx.sink.emit_stdout(line)?;
        self.observe_loop_line(line);
        ctx.output.output_buffer.push_str(line);
        ctx.output.output_buffer.push('\n');
        Ok(())
    }

    fn handle_stderr(&mut self, line: &str, ctx: &mut RunContext) -> Result<()> {
        // Parse MCP startup from stderr
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("mcp: ") {
            if let Some(server) = rest.strip_suffix(" starting") {
                ctx.emit(AgentEvent::McpServerUpdate {
                    server: server.trim().to_string(),
                    status: McpStatus::Starting,
                })?;
                ctx.render(RenderLine::mcp(format!("mcp: {server} starting")))?;
                return Ok(());
            }
            if let Some(server) = rest.strip_suffix(" ready") {
                ctx.emit(AgentEvent::McpServerUpdate {
                    server: server.trim().to_string(),
                    status: McpStatus::Ready,
                })?;
                ctx.render(RenderLine::mcp(format!("mcp: {server} ready")))?;
                return Ok(());
            }
            if let Some((server, _)) = rest.split_once(" failed:") {
                ctx.emit(AgentEvent::McpServerUpdate {
                    server: server.trim().to_string(),
                    status: McpStatus::Failed,
                })?;
                ctx.render(RenderLine::error(line))?;
                return Ok(());
            }
        }

        if trimmed.starts_with("mcp startup:") {
            return Ok(());
        }

        ctx.sink.emit_stderr(line)?;
        Ok(())
    }

    fn handle_tick(&mut self, _ctx: &mut RunContext) -> Result<()> {
        Ok(())
    }

    fn finish(&mut self, ctx: &mut RunContext) -> Result<()> {
        if let Some(parser) = self.event_parser.as_mut() {
            let result = parser.flush();
            for render_line in result.lines {
                self.observe_loop_line(&render_line.text);
                ctx.render(render_line)?;
            }
            if let Some(latest) = result.latest_response {
                ctx.output.latest_ai_response = Some(latest);
            }
        }
        Ok(())
    }

    fn loop_feedback(&self, output: &str) -> LoopFeedback {
        if self.loop_mode != ClaudeLoopMode::RalphPlugin {
            return LoopFeedback::default();
        }

        let mut feedback = LoopFeedback::default();

        let outcome = self
            .loop_tracker
            .last_outcome
            .clone()
            .or_else(|| loop_state::detect_outcome(output));

        match outcome {
            Some(ClaudeLoopOutcome::PromiseDetected(_)) => {
                feedback.should_stop = true;
                feedback.reason = Some("Claude loop promise detected".to_string());
            }
            Some(ClaudeLoopOutcome::MaxIterations(max)) => {
                feedback.should_stop = true;
                feedback.reason = Some(format!("Claude loop reached max iterations ({max})"));
            }
            Some(ClaudeLoopOutcome::Warning(msg)) => {
                feedback.reason = Some(format!("Claude loop warning: {msg}"));
            }
            None => {}
        }

        feedback
    }

    fn resolve_latest_response(
        &self,
        exit_code: i32,
        output_buffer: &str,
        handler_latest: Option<String>,
        _captured_latest: Option<String>,
    ) -> Option<String> {
        handler_latest.or_else(|| {
            if exit_code == 0 {
                let fallback = output_buffer.trim();
                (!fallback.is_empty()).then(|| fallback.to_string())
            } else {
                None
            }
        })
    }
}
