//! CLI argument definitions for ralph-wiggum.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use ralph_core::types::{ApprovalPolicy, ClaudeLoopMode, ClaudeOutputFormat, SandboxMode};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser, Debug)]
#[command(name = "ralph-rs")]
#[command(version = VERSION)]
#[command(about = "Ralph Wiggum technique for iterative AI development loops")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// The prompt to send to the AI agent
    #[arg(value_name = "PROMPT")]
    pub prompt: Option<String>,

    /// Read the prompt from a file instead of the command line
    #[arg(long = "prompt-file", value_name = "FILE")]
    pub prompt_file: Option<PathBuf>,

    /// Maximum number of iterations (0 = unlimited)
    #[arg(short = 'n', long, default_value = "10")]
    pub max_iterations: u32,

    /// AI agent to use (codex, claude)
    #[arg(long, default_value = "codex")]
    pub agent: String,

    /// Model to use
    #[arg(long)]
    pub model: Option<String>,

    // === Codex Options ===
    /// Sandbox mode to use for Codex shell/file actions
    #[arg(long = "codex-sandbox", value_enum, default_value_t = SandboxMode::WorkspaceWrite)]
    pub codex_sandbox: SandboxMode,

    /// Approval policy to use for Codex shell/file actions
    #[arg(long = "codex-approval", value_enum, default_value_t = ApprovalPolicy::Never)]
    pub codex_approval: ApprovalPolicy,

    /// Resume the last Codex session
    #[arg(long)]
    pub codex_resume_last: bool,

    /// Resume a specific Codex session by ID
    #[arg(long)]
    pub codex_resume: Option<String>,

    /// Fork the last Codex session
    #[arg(long)]
    pub codex_fork_last: bool,

    /// Fork a specific Codex session by ID
    #[arg(long)]
    pub codex_fork: Option<String>,

    /// Additional directories Codex can write to
    #[arg(long = "codex-add-dir")]
    pub codex_add_dirs: Vec<PathBuf>,

    /// Images to attach to the Codex prompt
    #[arg(long = "image")]
    pub codex_images: Vec<PathBuf>,

    /// Enable web search for Codex
    #[arg(long)]
    pub codex_search: bool,

    /// Output schema file for Codex structured output
    #[arg(long = "output-schema")]
    pub codex_output_schema: Option<PathBuf>,

    // === OpenCode Options ===
    /// Continue the last OpenCode session
    #[arg(long)]
    pub opencode_continue: bool,

    /// OpenCode session ID to continue
    #[arg(long)]
    pub opencode_session: Option<String>,

    /// Fork the OpenCode session before continuing
    #[arg(long)]
    pub opencode_fork: bool,

    /// Files to attach to OpenCode message
    #[arg(long = "opencode-file")]
    pub opencode_files: Vec<PathBuf>,

    /// Title for the OpenCode session
    #[arg(long)]
    pub opencode_title: Option<String>,

    /// Attach to a running OpenCode server
    #[arg(long)]
    pub opencode_attach: Option<String>,

    /// Directory to run OpenCode in
    #[arg(long)]
    pub opencode_dir: Option<PathBuf>,

    /// Port for the local OpenCode server
    #[arg(long)]
    pub opencode_port: Option<u16>,

    /// Model variant for OpenCode (reasoning effort)
    #[arg(long)]
    pub opencode_variant: Option<String>,

    /// Show thinking blocks in OpenCode
    #[arg(long)]
    pub opencode_thinking: bool,

    /// OpenCode output format (default or json)
    #[arg(long)]
    pub opencode_format: Option<String>,

    /// Agent to use in OpenCode
    #[arg(long)]
    pub opencode_agent: Option<String>,

    // === Claude Options ===
    /// Claude output format
    #[arg(long = "claude-output", value_enum, default_value_t = ClaudeOutputFormat::StreamJson)]
    pub claude_output_format: ClaudeOutputFormat,

    /// Include partial messages in Claude output
    #[arg(long)]
    pub claude_include_partial_messages: bool,

    /// Replay user messages in Claude conversation
    #[arg(long)]
    pub claude_replay_user_messages: bool,

    /// Continue the previous Claude session
    #[arg(long = "claude-continue")]
    pub claude_continue: bool,

    /// Resume a specific Claude session by ID
    #[arg(long = "claude-resume")]
    pub claude_resume: Option<String>,

    /// Claude session ID to use
    #[arg(long = "claude-session-id")]
    pub claude_session_id: Option<String>,

    /// Fork the current Claude session
    #[arg(long = "claude-fork-session")]
    pub claude_fork_session: bool,

    /// Create PR from Claude session
    #[arg(long = "claude-from-pr")]
    pub claude_from_pr: Option<String>,

    /// Claude agent to use
    #[arg(long = "claude-agent")]
    pub claude_agent: Option<String>,

    /// Claude tools to enable
    #[arg(long = "claude-tools")]
    pub claude_tools: Option<String>,

    /// System prompt for Claude
    #[arg(long = "claude-system-prompt")]
    pub claude_system_prompt: Option<String>,

    /// Append to system prompt for Claude
    #[arg(long = "claude-append-system-prompt")]
    pub claude_append_system_prompt: Option<String>,

    /// System prompt file for Claude
    #[arg(long = "claude-system-prompt-file")]
    pub claude_system_prompt_file: Option<PathBuf>,

    /// Append system prompt file for Claude
    #[arg(long = "claude-append-system-prompt-file")]
    pub claude_append_system_prompt_file: Option<PathBuf>,

    /// Claude plugin directories
    #[arg(long = "claude-plugin-dir")]
    pub claude_plugin_dirs: Vec<PathBuf>,

    /// Claude print mode
    #[arg(long = "claude-print")]
    pub claude_print: bool,

    /// Additional directories for Claude
    #[arg(long = "claude-add-dir")]
    pub claude_add_dirs: Vec<PathBuf>,

    /// MCP config files for Claude
    #[arg(long = "claude-mcp-config")]
    pub claude_mcp_configs: Vec<PathBuf>,

    /// Skip permissions check (dangerous)
    #[arg(long = "claude-dangerously-skip-permissions")]
    pub claude_dangerously_skip_permissions: bool,

    /// Claude settings file
    #[arg(long = "claude-settings")]
    pub claude_settings: Option<PathBuf>,

    /// Claude setting sources
    #[arg(long = "claude-setting-sources")]
    pub claude_setting_sources: Option<String>,

    /// Maximum budget in USD for Claude
    #[arg(long = "claude-max-budget-usd")]
    pub claude_max_budget_usd: Option<f64>,

    /// Disallowed tools for Claude
    #[arg(long = "claude-disallowed-tools")]
    pub claude_disallowed_tools: Option<String>,

    /// Disable slash commands in Claude
    #[arg(long = "claude-disable-slash-commands")]
    pub claude_disable_slash_commands: bool,

    /// Enable MCP debug mode for Claude
    #[arg(long = "claude-mcp-debug")]
    pub claude_mcp_debug: bool,

    /// Enable debug mode for Claude
    #[arg(long = "claude-debug")]
    pub claude_debug: bool,

    /// Enable worktree mode for Claude
    #[arg(long = "claude-worktree")]
    pub claude_worktree: bool,

    /// Claude agents configuration
    #[arg(long = "claude-agents")]
    pub claude_agents: Option<String>,

    /// Initialize Claude
    #[arg(long = "claude-init")]
    pub claude_init: bool,

    /// Initialize Claude only (don't run)
    #[arg(long = "claude-init-only")]
    pub claude_init_only: bool,

    /// Run Claude maintenance
    #[arg(long = "claude-maintenance")]
    pub claude_maintenance: bool,

    /// Claude loop mode
    #[arg(long = "claude-loop-mode", value_enum, default_value_t = ClaudeLoopMode::Print)]
    pub claude_loop_mode: ClaudeLoopMode,

    // === Loop Options ===
    /// Always use one session (Codex)
    #[arg(long)]
    pub one_session: bool,

    /// Delay between iterations in seconds
    #[arg(long, default_value = "3")]
    pub delay: u64,

    /// Terminal promise for completion detection
    #[arg(long)]
    pub promise: Option<String>,

    /// Tasks file for completion detection
    #[arg(long)]
    pub tasks: Option<PathBuf>,

    /// Disable TUI mode (use plain text output)
    #[arg(long)]
    pub no_tui: bool,

    /// Resume from saved state
    #[arg(long)]
    pub resume: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Show current loop status
    Status,
    /// Stop the running loop
    Stop,
    /// Clear saved state
    Clear,
    /// Show version information
    Version,
}
