pub mod behavior;
pub mod parser;
pub mod plugin;
pub mod prompt;
pub mod runner;

pub use parser::{CodexEventParser, is_transient_codex_progress_line};
pub use plugin::CodexPlugin;
pub use runner::CodexRunner;

/// Create the Codex agent plugin.
pub fn plugin() -> Box<dyn ralph_core::plugin::AgentPlugin> {
    Box::new(CodexPlugin)
}
