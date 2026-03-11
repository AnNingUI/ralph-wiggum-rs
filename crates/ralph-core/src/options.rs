use std::path::PathBuf;

use crate::types::{ApprovalPolicy, ClaudeLoopMode, ClaudeOutputFormat, SandboxMode};

/// CLI-agnostic agent options. Built from CLI args by cli_mapper (ralph-cli).
/// Agent crates depend on this, not on clap.
#[derive(Debug, Clone, Default)]
pub struct AgentOptions {
    pub common: CommonOptions,
    pub codex: CodexOptions,
    pub claude: ClaudeOptions,
}

#[derive(Debug, Clone, Default)]
pub struct CommonOptions {
    pub allow_all_permissions: bool,
    pub extra_flags: Vec<String>,
    pub stream_output: bool,
    pub sandbox_mode: Option<SandboxMode>,
    pub approval_policy: Option<ApprovalPolicy>,
    pub extra_writable_dirs: Vec<PathBuf>,
    pub output_last_message_path: Option<PathBuf>,
    pub one_session: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CodexOptions {
    pub resume_last: bool,
    pub resume_session: Option<String>,
    pub fork_last: bool,
    pub fork_session: Option<String>,
    pub images: Vec<PathBuf>,
    pub search: bool,
    pub output_schema: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct ClaudeOptions {
    pub output_format: Option<ClaudeOutputFormat>,
    pub include_partial_messages: bool,
    pub replay_user_messages: bool,
    pub continue_session: bool,
    pub resume: Option<String>,
    pub session_id: Option<String>,
    pub fork_session: bool,
    pub from_pr: Option<String>,
    pub agent: Option<String>,
    pub tools: Option<String>,
    pub system_prompt: Option<String>,
    pub append_system_prompt: Option<String>,
    pub system_prompt_file: Option<PathBuf>,
    pub append_system_prompt_file: Option<PathBuf>,
    pub plugin_dirs: Vec<PathBuf>,
    pub print_mode: bool,
    pub add_dirs: Vec<PathBuf>,
    pub mcp_configs: Vec<PathBuf>,
    pub skip_permissions: bool,
    pub settings_file: Option<PathBuf>,
    pub setting_sources: Option<String>,
    pub max_budget_usd: Option<f64>,
    pub disallowed_tools: Option<String>,
    pub disable_slash_commands: bool,
    pub mcp_debug: bool,
    pub debug: bool,
    pub worktree: bool,
    pub agents: Option<String>,
    pub init: bool,
    pub init_only: bool,
    pub maintenance: bool,
    pub loop_mode: Option<ClaudeLoopMode>,
}

/// Iteration state passed to plugins.
#[derive(Debug, Clone)]
pub struct IterationState {
    pub iteration: u32,
    pub max_iterations: u32,
    pub prompt: String,
    pub model: String,
    pub project_dir: PathBuf,
}
