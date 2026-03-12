use ralph_core::options::AgentOptions;
use ralph_core::types::ClaudeOutputFormat;

/// Build Claude CLI arguments from AgentOptions.
pub fn build_claude_args(prompt: &str, model: &str, options: &AgentOptions) -> Vec<String> {
    let claude = &options.claude;

    let mut args = Vec::with_capacity(
        20 + claude.plugin_dirs.len() * 2
            + claude.add_dirs.len() * 2
            + claude.mcp_configs.len() * 2,
    );

    // Determine if we're in print mode
    let loop_mode = claude.loop_mode.unwrap_or(ralph_core::types::ClaudeLoopMode::Print);
    let is_print_mode = claude.print_mode || loop_mode == ralph_core::types::ClaudeLoopMode::Print;

    if is_print_mode {
        args.push("--print".to_string());
    }
    if let Some(system_prompt) = &claude.system_prompt {
        args.push("--system-prompt".to_string());
        args.push(system_prompt.clone());
    }
    if let Some(file) = &claude.system_prompt_file {
        args.push("--system-prompt-file".to_string());
        args.push(file.display().to_string());
    }
    if let Some(append) = &claude.append_system_prompt {
        args.push("--append-system-prompt".to_string());
        args.push(append.clone());
    }
    if let Some(file) = &claude.append_system_prompt_file {
        args.push("--append-system-prompt-file".to_string());
        args.push(file.display().to_string());
    }
    if claude.continue_session {
        args.push("--continue".to_string());
    }
    if let Some(resume) = &claude.resume {
        args.push("--resume".to_string());
        args.push(resume.clone());
    }
    if let Some(session_id) = &claude.session_id {
        args.push("--session-id".to_string());
        args.push(session_id.clone());
    }
    if claude.fork_session {
        args.push("--fork-session".to_string());
    }
    if let Some(from_pr) = &claude.from_pr {
        args.push("--from-pr".to_string());
        args.push(from_pr.clone());
    }
    if let Some(agent) = &claude.agent {
        args.push("--agent".to_string());
        args.push(agent.clone());
    }
    if let Some(tools) = &claude.tools {
        args.push("--tools".to_string());
        args.push(tools.clone());
    }
    if let Some(file) = &claude.settings_file {
        args.push("--settings".to_string());
        args.push(file.display().to_string());
    }
    if let Some(sources) = &claude.setting_sources {
        args.push("--setting-sources".to_string());
        args.push(sources.clone());
    }
    if let Some(budget) = claude.max_budget_usd {
        args.push("--max-budget-usd".to_string());
        args.push(budget.to_string());
    }
    if let Some(disallowed) = &claude.disallowed_tools {
        args.push("--disallowedTools".to_string());
        args.push(disallowed.clone());
    }
    if claude.disable_slash_commands {
        args.push("--disable-slash-commands".to_string());
    }
    if claude.mcp_debug {
        args.push("--mcp-debug".to_string());
    }
    if claude.debug {
        args.push("--debug".to_string());
    }
    if claude.worktree {
        args.push("--worktree".to_string());
    }
    if let Some(agents) = &claude.agents {
        args.push("--agents".to_string());
        args.push(agents.clone());
    }
    if claude.init {
        args.push("--init".to_string());
    }
    if claude.init_only {
        args.push("--init-only".to_string());
    }
    if claude.maintenance {
        args.push("--maintenance".to_string());
    }
    if let Some(format) = claude.output_format {
        // Don't set output format if using --print mode (they conflict)
        // In print mode, Claude only supports text output
        if !is_print_mode && format != ClaudeOutputFormat::Text {
            args.push(format!("--output-format={}", format.as_cli_value()));
        }
    }
    if claude.include_partial_messages {
        args.push("--include-partial-messages".to_string());
    }
    if claude.replay_user_messages {
        args.push("--replay-user-messages".to_string());
    }
    for dir in &claude.plugin_dirs {
        args.push("--plugin-dir".to_string());
        args.push(dir.display().to_string());
    }
    for dir in &claude.add_dirs {
        args.push("--add-dir".to_string());
        args.push(dir.display().to_string());
    }
    for config in &claude.mcp_configs {
        args.push("--mcp-config".to_string());
        args.push(config.display().to_string());
    }
    if claude.skip_permissions {
        args.push("--dangerously-skip-permissions".to_string());
    }
    if !model.trim().is_empty() {
        args.push("--model".to_string());
        args.push(model.to_string());
    }

    // Extra common flags
    args.extend_from_slice(&options.common.extra_flags);

    args.push(prompt.to_string());
    args
}

/// Claude command name (platform-specific).
pub fn claude_command() -> &'static str {
    if cfg!(target_os = "windows") {
        "claude.exe"
    } else {
        "claude"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ralph_core::options::{AgentOptions, ClaudeOptions};

    #[test]
    fn builds_basic_args() {
        let options = AgentOptions {
            claude: ClaudeOptions {
                output_format: Some(ClaudeOutputFormat::StreamJson),
                print_mode: false,
                loop_mode: Some(ralph_core::types::ClaudeLoopMode::RalphPlugin),
                ..Default::default()
            },
            ..Default::default()
        };
        let args = build_claude_args("fix bug", "claude-sonnet-4", &options);
        assert!(args.contains(&"--output-format=stream-json".to_string()));
        assert!(args.contains(&"--model".to_string()));
        assert!(args.contains(&"claude-sonnet-4".to_string()));
        assert!(args.last() == Some(&"fix bug".to_string()));
    }

    #[test]
    fn builds_print_mode_args() {
        let options = AgentOptions {
            claude: ClaudeOptions {
                print_mode: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let args = build_claude_args("test", "model", &options);
        assert!(args.contains(&"--print".to_string()));
    }
}
