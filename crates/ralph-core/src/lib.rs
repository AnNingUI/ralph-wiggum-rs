pub mod completion;
pub mod event;
pub mod options;
pub mod plugin;
pub mod progress;
pub mod render;
pub mod state;
pub mod status;
pub mod types;
pub mod utils;

// Re-exports: unified protocol
pub use event::{
    AgentEvent, Decision, LoopOutcomeKind, McpFailure, McpStatus, OutputStream, Role, ToolSource,
    ToolStatus,
};
pub use progress::{ActiveTool, McpStartupSummary, Phase, ProgressSnapshot, ProgressTracker};
pub use render::{RenderKind, RenderLine, RenderMode};
pub use status::{StatusMeta, StatusRenderer};

// Re-exports: traits
pub use plugin::{
    AgentPlugin, IterationPlan, LoopFeedback, LoopMode, Notice, NoticeLevel, OutputSink,
    OutputState, PlainOutput, RunContext, Runner,
};

// Re-exports: types
pub use types::{
    AgentType, ApprovalPolicy, ClaudeLoopMode, ClaudeOutputFormat, JsonAgentConfig, RalphConfig,
    SandboxMode,
};

// Re-exports: options
pub use options::{AgentOptions, ClaudeOptions, CodexOptions, CommonOptions, IterationState};

// Re-exports: state
pub use state::{
    AgentModelPair, History, IterationHistory, RalphState, clear_last_message_capture, clear_state,
    ensure_state_dir, get_context_path, get_history_path, get_last_message_capture_path,
    get_prev_ai_path, get_questions_path, get_state_dir, get_state_path, get_tasks_path,
    load_context, load_history, load_last_message_capture, load_prev_ai_response, load_state,
    load_tasks, save_context, save_history, save_prev_ai_response, save_state, save_tasks,
    state_exists,
};

// Re-exports: config path
pub use types::get_default_config_path;

// Re-exports: completion
pub use completion::{check_terminal_promise, strip_ansi, tasks_markdown_all_complete};

// Re-exports: utils
pub use utils::{
    PREV_AI_SYSTEM_PROMPT, command_exists, command_exists_blocking, format_duration,
    get_file_mtime, inject_prev_ai_context, truncate_string,
};

// Re-exports: status helpers
pub use status::{format_token_count, shorten_middle};
