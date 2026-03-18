use anyhow::Result;

use ralph_core::options::AgentOptions;
use ralph_core::plugin::{AgentPlugin, IterationPlan, LoopMode, Notice, Runner};
use ralph_core::types::{AgentType, ClaudeLoopMode, ClaudeOutputFormat};

use crate::runner::ClaudeRunner;

pub struct ClaudePlugin;

impl AgentPlugin for ClaudePlugin {
    fn agent_type(&self) -> AgentType {
        AgentType::ClaudeCode
    }

    fn name(&self) -> &str {
        "claude"
    }

    fn create_runner(&self, options: &AgentOptions) -> Result<Box<dyn Runner>> {
        let output_format = options
            .claude
            .output_format
            .unwrap_or(ClaudeOutputFormat::StreamJson);
        let loop_mode = options.claude.loop_mode.unwrap_or(ClaudeLoopMode::Print);
        let replay = options.claude.replay_user_messages;

        Ok(Box::new(ClaudeRunner::new(
            output_format,
            loop_mode,
            replay,
        )))
    }

    fn prepare_iteration(&self, _options: &AgentOptions) -> Result<Vec<Notice>> {
        Ok(Vec::new())
    }

    fn finish_iteration(&self, _options: &AgentOptions) -> Result<()> {
        Ok(())
    }

    fn plan_iteration(&self, options: &AgentOptions) -> Result<IterationPlan> {
        let _ = options;
        Ok(IterationPlan::Continue)
    }

    fn loop_mode(&self) -> LoopMode {
        LoopMode::External
    }
}
