use anyhow::{Result, anyhow};
use std::collections::HashMap;

use ralph_core::plugin::AgentPlugin;
use ralph_core::types::AgentType;

/// Central registry of agent plugins. Replaces the old AgentSystem.
pub struct AgentRegistry {
    plugins: HashMap<AgentType, Box<dyn AgentPlugin>>,
}

impl AgentRegistry {
    /// Create a registry with the built-in Codex and Claude plugins.
    pub fn new() -> Self {
        let mut registry = Self {
            plugins: HashMap::new(),
        };
        registry.register(ralph_codex::plugin());
        registry.register(ralph_claude::plugin());
        registry
    }

    pub fn register(&mut self, plugin: Box<dyn AgentPlugin>) {
        self.plugins.insert(plugin.agent_type(), plugin);
    }

    pub fn get(&self, agent_type: AgentType) -> Result<&dyn AgentPlugin> {
        self.plugins
            .get(&agent_type)
            .map(|p| p.as_ref())
            .ok_or_else(|| anyhow!("Agent type not registered: {}", agent_type))
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_codex_and_claude() {
        let registry = AgentRegistry::new();
        assert!(registry.get(AgentType::Codex).is_ok());
        assert!(registry.get(AgentType::ClaudeCode).is_ok());
    }

    #[test]
    fn registry_rejects_unknown() {
        let registry = AgentRegistry::new();
        assert!(registry.get(AgentType::Opencode).is_err());
    }
}
