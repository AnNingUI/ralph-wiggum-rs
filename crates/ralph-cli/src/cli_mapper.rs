//! Maps CLI arguments into AgentOptions.
//!
//! The Cli struct is defined in the binary crate (root main.rs). This module
//! provides a builder that constructs the CLI-agnostic `AgentOptions` from
//! individual values, so agent crates never depend on clap.

use std::path::PathBuf;

use ralph_core::options::{AgentOptions, ClaudeOptions, CodexOptions, CommonOptions, OpencodeOptions};
use ralph_core::types::{ApprovalPolicy, ClaudeLoopMode, ClaudeOutputFormat, SandboxMode};

/// Builder for constructing AgentOptions from CLI-level values.
#[derive(Debug, Clone, Default)]
pub struct CliOptionsBuilder {
    // common
    pub allow_all_permissions: bool,
    pub extra_flags: Vec<String>,
    pub stream_output: bool,
    pub sandbox_mode: Option<SandboxMode>,
    pub approval_policy: Option<ApprovalPolicy>,
    pub extra_writable_dirs: Vec<PathBuf>,
    pub output_last_message_path: Option<PathBuf>,
    pub one_session: bool,

    // codex
    pub codex_resume_last: bool,
    pub codex_resume_session: Option<String>,
    pub codex_fork_last: bool,
    pub codex_fork_session: Option<String>,
    pub codex_images: Vec<PathBuf>,
    pub codex_search: bool,
    pub codex_output_schema: Option<PathBuf>,

    // claude
    pub claude_output_format: Option<ClaudeOutputFormat>,
    pub claude_include_partial_messages: bool,
    pub claude_replay_user_messages: bool,
    pub claude_continue: bool,
    pub claude_resume: Option<String>,
    pub claude_session_id: Option<String>,
    pub claude_fork_session: bool,
    pub claude_from_pr: Option<String>,
    pub claude_agent: Option<String>,
    pub claude_tools: Option<String>,
    pub claude_system_prompt: Option<String>,
    pub claude_append_system_prompt: Option<String>,
    pub claude_system_prompt_file: Option<PathBuf>,
    pub claude_append_system_prompt_file: Option<PathBuf>,
    pub claude_plugin_dirs: Vec<PathBuf>,
    pub claude_print_mode: bool,
    pub claude_add_dirs: Vec<PathBuf>,
    pub claude_mcp_configs: Vec<PathBuf>,
    pub claude_skip_permissions: bool,
    pub claude_settings_file: Option<PathBuf>,
    pub claude_setting_sources: Option<String>,
    pub claude_max_budget_usd: Option<f64>,
    pub claude_disallowed_tools: Option<String>,
    pub claude_disable_slash_commands: bool,
    pub claude_mcp_debug: bool,
    pub claude_debug: bool,
    pub claude_worktree: bool,
    pub claude_agents: Option<String>,
    pub claude_init: bool,
    pub claude_init_only: bool,
    pub claude_maintenance: bool,
    pub claude_loop_mode: Option<ClaudeLoopMode>,

    // opencode
    pub opencode_continue: bool,
    pub opencode_session: Option<String>,
    pub opencode_fork: bool,
    pub opencode_files: Vec<PathBuf>,
    pub opencode_title: Option<String>,
    pub opencode_attach: Option<String>,
    pub opencode_dir: Option<PathBuf>,
    pub opencode_port: Option<u16>,
    pub opencode_variant: Option<String>,
    pub opencode_thinking: bool,
    pub opencode_format: Option<String>,
    pub opencode_agent: Option<String>,
}

impl CliOptionsBuilder {
    pub fn build(self) -> AgentOptions {
        AgentOptions {
            common: CommonOptions {
                allow_all_permissions: self.allow_all_permissions,
                extra_flags: self.extra_flags,
                stream_output: self.stream_output,
                sandbox_mode: self.sandbox_mode,
                approval_policy: self.approval_policy,
                extra_writable_dirs: self.extra_writable_dirs,
                output_last_message_path: self.output_last_message_path,
                one_session: self.one_session,
            },
            codex: CodexOptions {
                resume_last: self.codex_resume_last,
                resume_session: self.codex_resume_session,
                fork_last: self.codex_fork_last,
                fork_session: self.codex_fork_session,
                images: self.codex_images,
                search: self.codex_search,
                output_schema: self.codex_output_schema,
            },
            claude: ClaudeOptions {
                output_format: self.claude_output_format,
                include_partial_messages: self.claude_include_partial_messages,
                replay_user_messages: self.claude_replay_user_messages,
                continue_session: self.claude_continue,
                resume: self.claude_resume,
                session_id: self.claude_session_id,
                fork_session: self.claude_fork_session,
                from_pr: self.claude_from_pr,
                agent: self.claude_agent,
                tools: self.claude_tools,
                system_prompt: self.claude_system_prompt,
                append_system_prompt: self.claude_append_system_prompt,
                system_prompt_file: self.claude_system_prompt_file,
                append_system_prompt_file: self.claude_append_system_prompt_file,
                plugin_dirs: self.claude_plugin_dirs,
                print_mode: self.claude_print_mode,
                add_dirs: self.claude_add_dirs,
                mcp_configs: self.claude_mcp_configs,
                skip_permissions: self.claude_skip_permissions,
                settings_file: self.claude_settings_file,
                setting_sources: self.claude_setting_sources,
                max_budget_usd: self.claude_max_budget_usd,
                disallowed_tools: self.claude_disallowed_tools,
                disable_slash_commands: self.claude_disable_slash_commands,
                mcp_debug: self.claude_mcp_debug,
                debug: self.claude_debug,
                worktree: self.claude_worktree,
                agents: self.claude_agents,
                init: self.claude_init,
                init_only: self.claude_init_only,
                maintenance: self.claude_maintenance,
                loop_mode: self.claude_loop_mode,
            },
            opencode: OpencodeOptions {
                continue_session: self.opencode_continue,
                session_id: self.opencode_session,
                fork_session: self.opencode_fork,
                files: self.opencode_files,
                title: self.opencode_title,
                attach: self.opencode_attach,
                dir: self.opencode_dir,
                port: self.opencode_port,
                variant: self.opencode_variant,
                thinking: self.opencode_thinking,
                format: self.opencode_format,
                agent: self.opencode_agent,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_produces_defaults() {
        let options = CliOptionsBuilder::default().build();
        assert!(!options.common.allow_all_permissions);
        assert!(!options.codex.resume_last);
        assert!(options.claude.output_format.is_none());
    }

    #[test]
    fn builder_maps_all_fields() {
        let options = CliOptionsBuilder {
            allow_all_permissions: true,
            sandbox_mode: Some(SandboxMode::DangerFullAccess),
            codex_resume_last: true,
            claude_print_mode: true,
            claude_loop_mode: Some(ClaudeLoopMode::RalphPlugin),
            ..Default::default()
        }
        .build();

        assert!(options.common.allow_all_permissions);
        assert_eq!(
            options.common.sandbox_mode,
            Some(SandboxMode::DangerFullAccess)
        );
        assert!(options.codex.resume_last);
        assert!(options.claude.print_mode);
        assert_eq!(
            options.claude.loop_mode,
            Some(ClaudeLoopMode::RalphPlugin)
        );
    }
}
