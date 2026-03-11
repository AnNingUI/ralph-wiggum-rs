pub mod behavior;
pub mod loop_state;
pub mod parser;
pub mod plugin;
pub mod plugin_catalog;
pub mod plugin_components;
pub mod plugin_summary;
pub mod plugin_workspace;
pub mod runner;
pub mod stream;

pub use parser::ClaudeEventParser;
pub use plugin::ClaudePlugin;
pub use runner::ClaudeRunner;
pub use stream::ClaudeStreamParser;

/// Create the Claude agent plugin.
pub fn plugin() -> Box<dyn ralph_core::plugin::AgentPlugin> {
    Box::new(ClaudePlugin)
}
