use anyhow::Result;

use ralph_core::options::AgentOptions;
use ralph_core::plugin::{AgentPlugin, IterationPlan, LoopMode, Notice, Runner};
use ralph_core::types::AgentType;

use crate::runner::CodexRunner;

pub struct CodexPlugin;

impl AgentPlugin for CodexPlugin {
    fn agent_type(&self) -> AgentType {
        AgentType::Codex
    }

    fn name(&self) -> &str {
        "codex"
    }

    fn create_runner(&self, _options: &AgentOptions) -> Result<Box<dyn Runner>> {
        Ok(Box::new(CodexRunner::new(true)))
    }

    fn prepare_iteration(&self, _options: &AgentOptions) -> Result<Vec<Notice>> {
        Ok(Vec::new())
    }

    fn finish_iteration(&self, _options: &AgentOptions) -> Result<()> {
        Ok(())
    }

    fn plan_iteration(&self, _options: &AgentOptions) -> Result<IterationPlan> {
        Ok(IterationPlan::Continue)
    }

    fn loop_mode(&self) -> LoopMode {
        LoopMode::External
    }
}
