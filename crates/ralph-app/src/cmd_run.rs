//! Run subcommand - execute the main Ralph Wiggum loop.

use anyhow::{Result, anyhow};
use colored::Colorize;
use std::env;

use ralph_cli::{
    AgentRegistry, CliOptionsBuilder, load_codex_config_defaults,
    resolve_model, run_loop, validate_codex_resume, validate_non_codex_first_iteration,
};
use ralph_core::options::AgentOptions;
use ralph_core::plugin::{OutputSink, PlainOutput};
use ralph_core::state::{RalphState, load_state, save_state};
use ralph_core::status::StatusMeta;
use ralph_core::types::AgentType;
use ralph_tui::TuiOutput;

use crate::cli::Cli;

pub async fn run_main(cli: Cli) -> Result<()> {
    // Parse agent type
    let agent_type = cli.agent.parse::<AgentType>()?;

    // Load Codex config defaults
    let codex_config = load_codex_config_defaults();

    // Resolve model
    let resolved_model = resolve_model(agent_type, cli.model.as_deref(), &codex_config);

    // Build AgentOptions from CLI
    let options = build_options(&cli, &codex_config)?;

    // Load or create state
    let mut state = if cli.resume {
        load_state()?
    } else {
        create_initial_state(&cli, agent_type)?
    };

    // Validate options
    validate_codex_resume(agent_type, &options, state.iteration, state.one_session)?;
    validate_non_codex_first_iteration(agent_type, &options, state.iteration, state.one_session)?;

    // Get project directory
    let project_dir = env::current_dir()?;

    // Create agent registry
    let registry = AgentRegistry::new();
    let plugin = registry.get(agent_type)?;

    // Create status meta
    let meta = StatusMeta {
        agent: agent_type.as_str().to_string(),
        model: resolved_model.display_model.clone(),
        effort: resolved_model.reasoning_effort.clone().unwrap_or_default(),
        project_path: project_dir.display().to_string(),
        iteration: state.iteration,
        max_iterations: state.max_iterations,
        started_at: std::time::Instant::now(),
    };

    // Create output sink
    let mut sink: Box<dyn OutputSink> = if !cli.no_tui {
        Box::new(TuiOutput::new(meta.clone())?)
    } else {
        Box::new(PlainOutput)
    };

    // Create runner factory
    let mut create_runner = || -> Result<Box<dyn ralph_core::plugin::Runner>> {
        plugin.create_runner(&options)
    };

    // Determine loop mode
    let loop_mode = plugin.loop_mode();

    // Run the loop
    let outcome = run_loop(
        &mut state,
        &mut create_runner,
        &options,
        &resolved_model,
        agent_type.as_str(),
        sink.as_mut(),
        &project_dir,
        cli.delay,
        loop_mode,
    )
    .await?;

    // Print outcome
    if outcome.completed {
        println!();
        println!("{}", "Loop completed successfully!".green().bold());
    } else {
        println!();
        println!("{}", "Loop interrupted".yellow());
    }

    println!("Total iterations: {}", outcome.total_iterations);
    println!("Total duration:   {}ms", outcome.total_duration_ms);

    Ok(())
}

fn create_initial_state(cli: &Cli, _agent_type: AgentType) -> Result<RalphState> {
    let prompt = if let Some(ref prompt) = cli.prompt {
        prompt.clone()
    } else if let Some(ref file) = cli.prompt_file {
        std::fs::read_to_string(file)?
    } else {
        return Err(anyhow!("No prompt provided. Use PROMPT or --prompt-file"));
    };

    let tasks_file = cli.tasks.as_ref().map(|p| p.display().to_string());

    let state = RalphState {
        prompt,
        iteration: 1,
        max_iterations: cli.max_iterations,
        started_at: chrono::Utc::now(),
        one_session: cli.one_session,
        codex_resume_session: None,
        rotation: None,
        rotation_index: None,
        promise: cli.promise.clone(),
        tasks_file,
        questions_file: None,
    };

    save_state(&state)?;
    Ok(state)
}

fn build_options(cli: &Cli, _codex_config: &ralph_cli::CodexConfigDefaults) -> Result<AgentOptions> {
    let builder = CliOptionsBuilder {
        // Common
        allow_all_permissions: false,
        extra_flags: Vec::new(),
        stream_output: true,
        sandbox_mode: Some(cli.codex_sandbox),
        approval_policy: Some(cli.codex_approval),
        extra_writable_dirs: cli.codex_add_dirs.clone(),
        output_last_message_path: None,
        one_session: cli.one_session,

        // Codex
        codex_resume_last: cli.codex_resume_last,
        codex_resume_session: cli.codex_resume.clone(),
        codex_fork_last: cli.codex_fork_last,
        codex_fork_session: cli.codex_fork.clone(),
        codex_images: cli.codex_images.clone(),
        codex_search: cli.codex_search,
        codex_output_schema: cli.codex_output_schema.clone(),

        // Claude
        claude_output_format: Some(cli.claude_output_format),
        claude_include_partial_messages: cli.claude_include_partial_messages,
        claude_replay_user_messages: cli.claude_replay_user_messages,
        claude_continue: cli.claude_continue,
        claude_resume: cli.claude_resume.clone(),
        claude_session_id: cli.claude_session_id.clone(),
        claude_fork_session: cli.claude_fork_session,
        claude_from_pr: cli.claude_from_pr.clone(),
        claude_agent: cli.claude_agent.clone(),
        claude_tools: cli.claude_tools.clone(),
        claude_system_prompt: cli.claude_system_prompt.clone(),
        claude_append_system_prompt: cli.claude_append_system_prompt.clone(),
        claude_system_prompt_file: cli.claude_system_prompt_file.clone(),
        claude_append_system_prompt_file: cli.claude_append_system_prompt_file.clone(),
        claude_plugin_dirs: cli.claude_plugin_dirs.clone(),
        claude_print_mode: cli.claude_print,
        claude_add_dirs: cli.claude_add_dirs.clone(),
        claude_mcp_configs: cli.claude_mcp_configs.clone(),
        claude_skip_permissions: cli.claude_dangerously_skip_permissions,
        claude_settings_file: cli.claude_settings.clone(),
        claude_setting_sources: cli.claude_setting_sources.clone(),
        claude_max_budget_usd: cli.claude_max_budget_usd,
        claude_disallowed_tools: cli.claude_disallowed_tools.clone(),
        claude_disable_slash_commands: cli.claude_disable_slash_commands,
        claude_mcp_debug: cli.claude_mcp_debug,
        claude_debug: cli.claude_debug,
        claude_worktree: cli.claude_worktree,
        claude_agents: cli.claude_agents.clone(),
        claude_init: cli.claude_init,
        claude_init_only: cli.claude_init_only,
        claude_maintenance: cli.claude_maintenance,
        claude_loop_mode: Some(cli.claude_loop_mode),
    };

    Ok(builder.build())
}
