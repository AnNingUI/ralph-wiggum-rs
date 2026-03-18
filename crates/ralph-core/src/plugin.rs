use anyhow::Result;
use std::path::PathBuf;

use crate::options::AgentOptions;
use crate::types::AgentType;

/// Single registration point for an agent backend.
/// Replaces the previous AgentModule + AgentCliAdapter + AgentIntegration.
pub trait AgentPlugin: Send + Sync {
    fn agent_type(&self) -> AgentType;
    fn name(&self) -> &str;

    /// Create a Runner for this agent.
    fn create_runner(&self, options: &AgentOptions) -> Result<Box<dyn Runner>>;

    /// Pre-iteration hook: health checks, workspace prep, etc.
    fn prepare_iteration(&self, _options: &AgentOptions) -> Result<Vec<Notice>> {
        Ok(Vec::new())
    }

    /// Post-iteration hook: cleanup, state saving, etc.
    fn finish_iteration(&self, _options: &AgentOptions) -> Result<()> {
        Ok(())
    }

    /// Plan the next iteration (e.g., decide prompt, check completion).
    fn plan_iteration(&self, _options: &AgentOptions) -> Result<IterationPlan> {
        Ok(IterationPlan::Continue)
    }

    /// Agent-specific loop mode.
    fn loop_mode(&self) -> LoopMode {
        LoopMode::External
    }
}

/// Runner trait: handles stdout/stderr parsing and emits AgentEvent via RunContext.
pub trait Runner: Send {
    fn command(&self) -> &str;
    fn wants_stdin_prompt(&self) -> bool {
        false
    }
    fn output_capture_path(&self) -> Option<PathBuf> {
        None
    }
    fn tick_interval(&self) -> Option<std::time::Duration> {
        None
    }

    /// Whether this runner needs streaming JSON parsing.
    ///
    /// Returns true for agents that output stream-json format (like Claude Code).
    /// When true, the execution engine will use incremental JSON parsing
    /// instead of line-based parsing.
    fn needs_streaming_json(&self) -> bool {
        false
    }

    fn prepare_prompt(&self, prompt: &str) -> String {
        prompt.to_string()
    }

    fn build_args(&self, prompt: &str, model: &str, options: &AgentOptions) -> Vec<String>;

    fn build_env(&self, options: &AgentOptions) -> std::collections::HashMap<String, String> {
        let _ = options;
        std::collections::HashMap::new()
    }

    fn before_spawn(&mut self, _prompt: &str, _ctx: &mut RunContext) -> Result<()> {
        Ok(())
    }

    /// Parse one stdout line, emit events via ctx.emit() and render lines via ctx.render().
    fn handle_stdout(&mut self, line: &str, ctx: &mut RunContext) -> Result<()>;

    /// Parse one stderr line.
    fn handle_stderr(&mut self, line: &str, ctx: &mut RunContext) -> Result<()>;

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
    ) -> Option<String>;
}

/// Context passed to Runner methods. Carries ProgressTracker + OutputSink.
pub struct RunContext<'a> {
    pub output: &'a mut OutputState,
    pub sink: &'a mut dyn OutputSink,
    pub progress: &'a mut crate::progress::ProgressTracker,
    pub project_dir: &'a std::path::Path,
}

impl RunContext<'_> {
    /// Emit an event: updates progress tracker and routes to output sink.
    pub fn emit(&mut self, event: crate::event::AgentEvent) -> Result<()> {
        if let crate::event::AgentEvent::SessionStarted {
            session_id: Some(session_id),
        } = &event
        {
            self.output.session_id = Some(session_id.clone());
        }
        self.progress.observe(&event);
        self.sink.on_event(&event, self.progress.snapshot())?;
        Ok(())
    }

    /// Emit a render line to the output sink.
    pub fn render(&mut self, line: crate::render::RenderLine) -> Result<()> {
        self.sink.render_line(&line)?;
        Ok(())
    }
}

/// Accumulated output state during a run.
#[derive(Debug, Default)]
pub struct OutputState {
    pub output_buffer: String,
    pub tools_used: std::collections::HashMap<String, u32>,
    pub latest_ai_response: Option<String>,
    pub session_id: Option<String>,
    pub errors: Vec<String>,
}

/// Output sink trait: receives events and render lines.
pub trait OutputSink: Send {
    fn emit_stdout(&mut self, line: &str) -> Result<()>;
    fn emit_stderr(&mut self, line: &str) -> Result<()>;
    fn render_line(&mut self, line: &crate::render::RenderLine) -> Result<()>;

    fn on_event(
        &mut self,
        _event: &crate::event::AgentEvent,
        _snapshot: &crate::progress::ProgressSnapshot,
    ) -> Result<()> {
        Ok(())
    }

    fn set_status(&mut self, _status: Option<String>) -> Result<()> {
        Ok(())
    }

    fn set_meta(&mut self, _meta: &crate::status::StatusMeta) -> Result<()> {
        Ok(())
    }

    /// Check if the user has requested an interrupt (e.g. ESC in TUI mode).
    /// Returns `true` if the execution should be aborted.
    fn check_interrupt(&mut self) -> Result<bool> {
        Ok(false)
    }
}

/// Simple PlainOutput: prints to stdout/stderr with colored formatting.
pub struct PlainOutput;

impl OutputSink for PlainOutput {
    fn emit_stdout(&mut self, line: &str) -> Result<()> {
        println!("{line}");
        Ok(())
    }

    fn emit_stderr(&mut self, line: &str) -> Result<()> {
        eprintln!("{line}");
        Ok(())
    }

    fn render_line(&mut self, line: &crate::render::RenderLine) -> Result<()> {
        use crate::render::RenderKind;
        use colored::Colorize;
        match line.kind {
            RenderKind::Assistant => println!("{}", line.text),
            RenderKind::Reasoning => println!("{}", line.text.dimmed()),
            RenderKind::ToolCall => println!("{}", line.text.cyan()),
            RenderKind::ToolOutput | RenderKind::ToolOutputDelta => {
                println!("{}", line.text.dimmed())
            }
            RenderKind::Status | RenderKind::Progress | RenderKind::Mcp | RenderKind::Subagent => {
                println!("{}", line.text.dimmed())
            }
            RenderKind::Error => eprintln!("{}", line.text.red()),
            RenderKind::Todo => println!("{}", line.text.yellow()),
            RenderKind::Approval => println!("{}", line.text.magenta()),
        }
        Ok(())
    }
}

/// Iteration outcome.
#[derive(Debug, Clone)]
pub enum IterationPlan {
    Continue,
    Stop { reason: String },
}

/// Pre-iteration notice.
#[derive(Debug, Clone)]
pub struct Notice {
    pub level: NoticeLevel,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
}

/// Loop mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopMode {
    /// Ralph drives the outer loop.
    External,
    /// Agent handles its own internal loop (e.g. Claude ralph-plugin).
    Internal,
}

/// Feedback from the runner after an iteration.
#[derive(Debug, Clone, Default)]
pub struct LoopFeedback {
    pub should_stop: bool,
    pub reason: Option<String>,
}
