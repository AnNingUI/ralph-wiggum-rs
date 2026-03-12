pub mod behavior;
pub mod loop_state;
pub mod parser;
pub mod plugin;
pub mod plugin_catalog;
pub mod plugin_components;
pub mod plugin_summary;
pub mod plugin_workspace;
pub mod prompt;
pub mod runner;
pub mod stream;

pub use parser::OpencodeEventParser;
pub use plugin::OpencodePlugin;
pub use runner::OpencodeRunner;
pub use stream::OpencodeStreamParser;

/// Create the OpenCode agent plugin.
pub fn plugin() -> Box<dyn ralph_core::plugin::AgentPlugin> {
    Box::new(OpencodePlugin)
}
