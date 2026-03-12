use anyhow::Result;

use ralph_core::options::AgentOptions;
use ralph_core::plugin::{AgentPlugin, IterationPlan, LoopMode, Notice, Runner};
use ralph_core::types::AgentType;

use crate::runner::OpencodeRunner;

pub struct OpencodePlugin;

impl AgentPlugin for OpencodePlugin {
    fn agent_type(&self) -> AgentType {
        AgentType::Opencode
    }

    fn name(&self) -> &str {
        "opencode"
    }

    fn create_runner(&self, _options: &AgentOptions) -> Result<Box<dyn Runner>> {
        Ok(Box::new(OpencodeRunner::new(true)))
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
