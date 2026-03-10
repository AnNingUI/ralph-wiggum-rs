//! Ralph Wiggum Loop for AI agents
//!
//! Implementation of the Ralph Wiggum technique - continuous self-referential
//! AI loops for iterative development. Based on ghuntley.com/ralph/

use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use colored::*;
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::{Duration, MissedTickBehavior, interval, sleep};

use ralph_wiggum_rs::{
    CodexJsonEventProcessor, CodexProgressSnapshot, CodexRenderLine, CodexRenderLineKind,
    CodexUiEvent, IterationHistory, RalphState,
    agent::{
        AgentBuildArgsOptions, AgentEnvOptions, AgentType, ApprovalPolicy, SandboxMode,
        create_default_agent,
    },
    check_terminal_promise, format_duration, inject_prev_ai_context,
    state::*,
    tasks_markdown_all_complete,
    tui::{CodexTui, CodexTuiMeta, TuiInputAction},
};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const CODEX_FILE_EDIT_SYSTEM_PROMPT: &str = "For file edits, use the built-in apply_patch tool directly. Do not invoke apply_patch through shell/exec_command. Do not rely on shell multiline redirection, heredoc, or herestring to write files.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum CodexRenderMode {
    Plain,
    Rich,
    Tui,
    JsonPass,
    EventJson,
}

#[derive(Parser)]
#[command(name = "ralph")]
#[command(version = VERSION)]
#[command(about = "Ralph Wiggum technique for iterative AI development loops", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// The prompt to send to the AI agent
    #[arg(value_name = "PROMPT")]
    prompt: Option<String>,

    /// Read the prompt from a file instead of the command line
    #[arg(long = "prompt-file", value_name = "FILE")]
    prompt_file: Option<PathBuf>,

    /// Maximum number of iterations
    #[arg(short = 'n', long, default_value = "10")]
    max_iterations: u32,

    /// AI agent to use (opencode, claude-code, codex, copilot)
    #[arg(long, default_value = "opencode")]
    agent: String,

    /// Model to use
    #[arg(long)]
    model: Option<String>,

    /// Sandbox mode to use for Codex shell/file actions
    #[arg(long = "codex-sandbox", value_enum, default_value_t = SandboxMode::WorkspaceWrite)]
    codex_sandbox: SandboxMode,

    /// Approval policy to use for Codex shell/file actions
    #[arg(long = "codex-approval", value_enum, default_value_t = ApprovalPolicy::Never)]
    codex_approval: ApprovalPolicy,

    /// Additional writable directories to expose to Codex
    #[arg(long = "codex-add-dir", value_name = "DIR")]
    codex_add_dirs: Vec<PathBuf>,

    /// Codex output rendering mode
    #[arg(long = "codex-render", value_enum, default_value_t = CodexRenderMode::Tui)]
    codex_render: CodexRenderMode,

    /// Promise tag for completion detection
    #[arg(long)]
    promise: Option<String>,

    /// Path to tasks markdown file for completion detection
    #[arg(long)]
    tasks: Option<String>,

    /// Enable agent rotation (comma-separated: agent1:model1,agent2:model2)
    #[arg(long)]
    rotation: Option<String>,

    /// Delay between iterations in seconds
    #[arg(long, default_value = "2")]
    delay: u64,

    /// Resume from existing state
    #[arg(long)]
    resume: bool,

    /// Resume a previous Codex exec session (applies to iteration 1 only)
    #[arg(
        long = "codex-resume",
        value_name = "SESSION_ID",
        conflicts_with = "codex_resume_last"
    )]
    codex_resume: Option<String>,

    /// Resume the most recent Codex exec session (applies to iteration 1 only)
    #[arg(
        long = "codex-resume-last",
        default_value_t = false,
        conflicts_with = "codex_resume"
    )]
    codex_resume_last: bool,

    /// Reuse the same Codex session across iterations (persist resume id)
    #[arg(long = "one-session", default_value_t = false)]
    one_session: bool,

    /// Additional context to inject
    #[arg(long)]
    context: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Show current loop status
    Status,
    /// Stop the current loop
    Stop,
    /// Clear all state
    Clear,
    /// Show version information
    Version,
}

#[derive(Debug, Clone)]
struct ResolvedModelSelection {
    execution_model: String,
    display_model: String,
    reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct CodexConfigDefaults {
    model: Option<String>,
    reasoning_effort: Option<String>,
}

#[derive(Debug, Clone)]
struct CodexStatusMeta {
    model: String,
    reasoning_effort: String,
    project_path: String,
    iteration: u32,
    max_iterations: u32,
    iteration_started_at: Instant,
}

fn normalize_model_name(model: Option<&str>) -> Option<String> {
    model
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(ToOwned::to_owned)
}

fn resolve_model_selection(
    agent_type: AgentType,
    requested_model: Option<&str>,
    codex_config: &CodexConfigDefaults,
) -> ResolvedModelSelection {
    if let Some(explicit_model) = normalize_model_name(requested_model) {
        return ResolvedModelSelection {
            execution_model: explicit_model.clone(),
            display_model: explicit_model,
            reasoning_effort: (agent_type == AgentType::Codex)
                .then(|| codex_config.reasoning_effort.clone())
                .flatten(),
        };
    }

    if let Some(default_model) = agent_type.default_model() {
        let default_model = default_model.to_string();
        return ResolvedModelSelection {
            execution_model: default_model.clone(),
            display_model: default_model,
            reasoning_effort: None,
        };
    }

    if agent_type == AgentType::Codex {
        if let Some(configured_model) = codex_config.model.clone() {
            return ResolvedModelSelection {
                execution_model: String::new(),
                display_model: configured_model,
                reasoning_effort: codex_config.reasoning_effort.clone(),
            };
        }
    }

    ResolvedModelSelection {
        execution_model: String::new(),
        display_model: agent_type.implicit_model_label().to_string(),
        reasoning_effort: None,
    }
}

fn prepend_codex_file_edit_prompt(prompt: &str) -> String {
    format!(
        "<system>{}</system>\n\n{}",
        CODEX_FILE_EDIT_SYSTEM_PROMPT, prompt
    )
}

fn find_codex_home() -> Option<PathBuf> {
    if let Ok(path) = env::var("CODEX_HOME") {
        let path = PathBuf::from(path);
        if path.is_dir() {
            return path.canonicalize().ok().or(Some(path));
        }
        return None;
    }

    env::var("USERPROFILE")
        .or_else(|_| env::var("HOME"))
        .ok()
        .map(PathBuf::from)
        .map(|path| path.join(".codex"))
}

fn parse_top_level_toml_string(contents: &str, key: &str) -> Option<String> {
    let mut in_root = true;

    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            in_root = false;
            continue;
        }

        if !in_root {
            continue;
        }

        let Some((candidate_key, candidate_value)) = line.split_once('=') else {
            continue;
        };
        if candidate_key.trim() != key {
            continue;
        }

        let candidate_value = candidate_value.split('#').next().unwrap_or_default().trim();

        if candidate_value.is_empty() {
            return None;
        }

        if candidate_value.starts_with('"')
            && candidate_value.ends_with('"')
            && candidate_value.len() >= 2
        {
            return Some(candidate_value[1..candidate_value.len() - 1].to_string());
        }

        return Some(candidate_value.to_string());
    }

    None
}

fn load_codex_config_defaults() -> CodexConfigDefaults {
    let Some(codex_home) = find_codex_home() else {
        return CodexConfigDefaults::default();
    };

    let config_path = codex_home.join("config.toml");
    let Ok(contents) = std::fs::read_to_string(config_path) else {
        return CodexConfigDefaults::default();
    };

    CodexConfigDefaults {
        model: parse_top_level_toml_string(&contents, "model"),
        reasoning_effort: parse_top_level_toml_string(&contents, "model_reasoning_effort"),
    }
}

fn shorten_middle(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars || max_chars <= 5 {
        return value.to_string();
    }

    let head = max_chars / 2;
    let tail = max_chars.saturating_sub(head + 3);
    let prefix = value.chars().take(head).collect::<String>();
    let suffix = value
        .chars()
        .rev()
        .take(tail)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("{}...{}", prefix, suffix)
}

fn format_token_count(value: i64) -> String {
    match value {
        1_000_000.. => format!("{:.1}m", value as f64 / 1_000_000.0),
        10_000.. => format!("{:.1}k", value as f64 / 1_000.0),
        1_000.. => format!("{:.0}k", value as f64 / 1_000.0),
        _ => value.to_string(),
    }
}

fn format_codex_token_summary(snapshot: &CodexProgressSnapshot) -> Option<String> {
    Some(format!(
        "tok in {} | cached {} | out {}",
        format_token_count(snapshot.input_tokens?),
        format_token_count(snapshot.cached_input_tokens?),
        format_token_count(snapshot.output_tokens?),
    ))
}

fn render_codex_iteration_panel(meta: &CodexStatusMeta) {
    println!(
        "{}",
        "   +-- Codex Session --------------------------------------".cyan()
    );
    println!(
        "{}",
        format!(
            "   | model  {}   effort  {}   loop  {}/{}",
            meta.model, meta.reasoning_effort, meta.iteration, meta.max_iterations
        )
        .dimmed()
    );
    println!(
        "{}",
        format!("   | path   {}", shorten_middle(&meta.project_path, 60)).dimmed()
    );
    println!(
        "{}",
        "   +--------------------------------------------------------".cyan()
    );
}

fn build_codex_status_line(meta: &CodexStatusMeta, snapshot: &CodexProgressSnapshot) -> String {
    let header = snapshot
        .status_header
        .as_deref()
        .map(str::trim)
        .filter(|header| !header.is_empty())
        .unwrap_or("Working");
    build_codex_status_line_with_header(meta, Some(snapshot), header)
}

fn codex_snapshot_should_show_footer(snapshot: &CodexProgressSnapshot) -> bool {
    matches!(
        snapshot.phase.as_str(),
        "thread" | "thinking" | "planning" | "reasoning" | "tool"
    )
}

fn build_codex_status_line_with_header(
    meta: &CodexStatusMeta,
    snapshot: Option<&CodexProgressSnapshot>,
    header: &str,
) -> String {
    let elapsed = format_duration(meta.iteration_started_at.elapsed().as_millis() as u64);
    let spinner = codex_spinner_frame(meta.iteration_started_at.elapsed().as_millis());
    let mut priority_segments = Vec::new();
    let mut optional_segments = Vec::new();
    let mut token_segment = None;

    if let Some(snapshot) = snapshot {
        if let Some(token_summary) = format_codex_token_summary(snapshot) {
            token_segment = Some(token_summary);
        }
        if snapshot.todo_total > 0 {
            optional_segments.push(format!(
                "todo {}/{}",
                snapshot.todo_completed, snapshot.todo_total
            ));
        }
        if snapshot.tool_calls > 0 {
            optional_segments.push(format!("tools {}", snapshot.tool_calls));
        }
        if let Some(last_tool) = snapshot
            .last_tool
            .as_deref()
            .filter(|tool| !tool.is_empty())
        {
            optional_segments.push(format!("last {last_tool}"));
        }
        if let Some(detail) = snapshot
            .last_detail
            .as_deref()
            .map(str::trim)
            .filter(|detail| !detail.is_empty() && *detail != header)
        {
            optional_segments.push(shorten_middle(detail, 28));
        }
        if let Some(last_error) = snapshot
            .last_error
            .as_deref()
            .map(str::trim)
            .filter(|error| !error.is_empty())
        {
            optional_segments.push(format!("err {}", shorten_middle(last_error, 20)));
        }
    }

    priority_segments.push(format!("model {}", meta.model));
    priority_segments.push(format!("effort {}", meta.reasoning_effort));
    priority_segments.push(format!("loop {}/{}", meta.iteration, meta.max_iterations));
    priority_segments.push(format!("path {}", shorten_middle(&meta.project_path, 12)));
    if let Some(token_segment) = token_segment {
        priority_segments.push(token_segment);
    }

    let mut line = format!("{spinner} {header} ({elapsed}s • ctrl+c to interrupt)");
    let max_chars = codex_status_max_chars();

    for segment in priority_segments
        .into_iter()
        .chain(optional_segments.into_iter())
    {
        let next_len = line.chars().count() + 3 + segment.chars().count();
        if next_len <= max_chars {
            line.push_str(" · ");
            line.push_str(&segment);
        }
    }

    if line.chars().count() > max_chars {
        shorten_middle(&line, max_chars)
    } else {
        line
    }
}

fn codex_status_max_chars() -> usize {
    #[cfg(test)]
    {
        200
    }

    #[cfg(not(test))]
    {
        env::var("COLUMNS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .or_else(|| {
                crossterm::terminal::size()
                    .ok()
                    .map(|(width, _)| width as usize)
            })
            .map(|width| width.saturating_sub(6).clamp(52, 160))
            .unwrap_or(96)
    }
}

fn codex_spinner_frame(elapsed_ms: u128) -> &'static str {
    const FRAMES: [&str; 4] = ["-", "\\", "|", "/"];
    let frame = ((elapsed_ms / 120) as usize) % FRAMES.len();
    FRAMES[frame]
}

fn is_transient_codex_progress_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }

    if !trimmed.contains("to interrupt") {
        return false;
    }

    let has_spinner_prefix = ['◐', '◓', '◑', '◒', '•']
        .iter()
        .any(|prefix| trimmed.starts_with(*prefix));
    let has_elapsed_segment = trimmed.contains('(')
        && trimmed.contains(')')
        && (trimmed.contains("ms") || trimmed.contains('s'));

    has_spinner_prefix || has_elapsed_segment
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum McpStartupState {
    Starting,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Default)]
struct McpStartupTracker {
    servers: BTreeMap<String, McpStartupState>,
}

impl McpStartupTracker {
    fn observe_line(&mut self, line: &str) -> bool {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("mcp: ") {
            if let Some(server) = rest.strip_suffix(" starting") {
                self.servers
                    .insert(server.trim().to_string(), McpStartupState::Starting);
                return true;
            }
            if let Some(server) = rest.strip_suffix(" ready") {
                self.servers
                    .insert(server.trim().to_string(), McpStartupState::Ready);
                return true;
            }
            if let Some((server, _)) = rest.split_once(" failed:") {
                self.servers
                    .insert(server.trim().to_string(), McpStartupState::Failed);
                return true;
            }
        }

        if trimmed.starts_with("mcp startup:") {
            self.servers.clear();
            return true;
        }

        false
    }

    fn header(&self) -> Option<String> {
        let total = self.servers.len();
        if total == 0 {
            return None;
        }

        let mut starting: Vec<_> = self
            .servers
            .iter()
            .filter_map(|(name, state)| {
                matches!(state, McpStartupState::Starting).then_some(name.as_str())
            })
            .collect();

        if starting.is_empty() {
            return None;
        }

        starting.sort_unstable();
        if total == 1 {
            return Some(format!("Booting MCP server: {}", starting[0]));
        }

        let completed = total.saturating_sub(starting.len());
        let max_to_show = 3;
        let mut to_show: Vec<String> = starting
            .iter()
            .take(max_to_show)
            .map(|name| (*name).to_string())
            .collect();
        if starting.len() > max_to_show {
            to_show.push("...".to_string());
        }

        Some(format!(
            "Starting MCP servers ({completed}/{total}): {}",
            to_show.join(", ")
        ))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Status) => {
            show_status()?;
            return Ok(());
        }
        Some(Commands::Stop) => {
            stop_loop()?;
            return Ok(());
        }
        Some(Commands::Clear) => {
            clear_state()?;
            println!("{}", "Cleared all Ralph state".green());
            return Ok(());
        }
        Some(Commands::Version) => {
            println!("ralph-wiggum v{}", VERSION);
            return Ok(());
        }
        None => {}
    }

    // Main loop execution
    if cli.resume {
        if cli.one_session || cli.codex_resume_last || cli.codex_resume.is_some() {
            return Err(anyhow!(
                "--resume uses saved state; start a new loop to change --one-session or --codex-resume*"
            ));
        }
        if !state_exists() {
            return Err(anyhow!("No existing state to resume from"));
        }
        println!("{}", "Resuming from existing state...".cyan());
        run_ralph_loop(None, &cli).await?;
    } else {
        let prompt = match (&cli.prompt_file, &cli.prompt) {
            (Some(_), Some(_)) => {
                return Err(anyhow!("Use either PROMPT or --prompt-file, not both"));
            }
            (Some(path), None) => {
                let contents = std::fs::read_to_string(path).map_err(|err| {
                    anyhow!("Failed to read prompt file {}: {}", path.display(), err)
                })?;
                let trimmed = contents.trim_end_matches(&['\r', '\n'][..]);
                if trimmed.is_empty() {
                    return Err(anyhow!("Prompt file is empty"));
                }
                trimmed.to_string()
            }
            (None, Some(prompt)) => prompt.clone(),
            (None, None) => {
                return Err(anyhow!("Prompt is required (use PROMPT or --prompt-file)"));
            }
        };

        if state_exists() {
            return Err(anyhow!(
                "A Ralph loop is already running. Use --resume to continue or 'ralph stop' to clear."
            ));
        }

        run_ralph_loop(Some(prompt.clone()), &cli).await?;
    }

    Ok(())
}

async fn run_ralph_loop(prompt: Option<String>, cli: &Cli) -> Result<()> {
    let codex_config_defaults = load_codex_config_defaults();
    let project_path = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let mut state = if let Some(p) = prompt {
        let mut s = RalphState::new(p, cli.max_iterations);

        // Parse rotation if provided
        if let Some(rotation_str) = &cli.rotation {
            let pairs: Result<Vec<_>> = rotation_str
                .split(',')
                .map(|pair| {
                    let parts: Vec<&str> = pair.split(':').collect();
                    if parts.len() != 2 {
                        return Err(anyhow!("Invalid rotation format: {}", pair));
                    }
                    Ok(AgentModelPair {
                        agent: parts[0].to_string(),
                        model: parts[1].to_string(),
                    })
                })
                .collect();
            s.rotation = Some(pairs?);
            s.rotation_index = Some(0);
        }

        s.promise = cli.promise.clone();
        s.tasks_file = cli.tasks.clone();
        s.one_session = cli.one_session;
        if cli.one_session {
            s.codex_resume_session = cli.codex_resume.clone();
        }

        save_state(&s)?;
        s
    } else {
        load_state()?
    };

    let mut history = load_history()?;

    let wants_codex_resume = cli.codex_resume_last || cli.codex_resume.is_some();
    if wants_codex_resume && state.iteration != 1 {
        return Err(anyhow!(
            "--codex-resume* can only be used when starting a fresh loop (iteration 1)"
        ));
    }

    let mut codex_tui = if cli.codex_render == CodexRenderMode::Tui {
        Some(CodexTui::new(CodexTuiMeta {
            model: "booting".to_string(),
            reasoning_effort: "default".to_string(),
            project_path: project_path.display().to_string(),
            iteration: state.iteration,
            max_iterations: state.max_iterations,
        })?)
    } else {
        None
    };

    // Save context if provided
    if let Some(context) = &cli.context {
        save_context(context)?;
    }

    if let Some(tui) = codex_tui.as_mut() {
        tui.push_raw_stdout("Starting Ralph Wiggum loop...")?;
        tui.push_raw_stdout(format!("Prompt: {}", state.prompt))?;
        tui.push_raw_stdout(format!("Max iterations: {}", state.max_iterations))?;
    } else {
        println!("{}", "Starting Ralph Wiggum loop...".green().bold());
        println!("{}", format!("   Prompt: {}", state.prompt).dimmed());
        println!(
            "{}",
            format!("   Max iterations: {}", state.max_iterations).dimmed()
        );
        println!();
    }

    while state.iteration <= state.max_iterations {
        let iteration_start = Instant::now();

        if let Some(tui) = codex_tui.as_mut() {
            tui.push_raw_stdout(format!(
                "--- Iteration {}/{} ---",
                state.iteration, state.max_iterations
            ))?;
        } else {
            println!(
                "{}",
                format!(
                    "--- Iteration {}/{} ---",
                    state.iteration, state.max_iterations
                )
                .cyan()
                .bold()
            );
        }

        // Determine current agent and model
        let (current_agent, requested_model) = if let Some(pair) = state.get_current_agent_model() {
            (pair.agent, Some(pair.model))
        } else {
            (cli.agent.clone(), cli.model.clone())
        };

        let agent_type = AgentType::from_str(&current_agent)?;
        let resolved_model = resolve_model_selection(
            agent_type,
            requested_model.as_deref(),
            &codex_config_defaults,
        );

        let use_codex_resume = agent_type == AgentType::Codex
            && (state.one_session && state.codex_resume_session.is_some()
                || (state.iteration == 1 && wants_codex_resume));
        if (state.iteration == 1 && (state.one_session || wants_codex_resume))
            && agent_type != AgentType::Codex
        {
            return Err(anyhow!(
                "--codex-resume* and --one-session require --agent codex on the first iteration"
            ));
        }

        if let Some(tui) = codex_tui.as_mut() {
            tui.push_raw_stdout(format!(
                "Agent: {} / {}",
                current_agent, resolved_model.display_model
            ))?;
        } else {
            println!(
                "{}",
                format!(
                    "   Agent: {} / {}",
                    current_agent, resolved_model.display_model
                )
                .dimmed()
            );
        }

        let codex_status_meta = if agent_type == AgentType::Codex
            && matches!(
                cli.codex_render,
                CodexRenderMode::Rich | CodexRenderMode::Tui
            ) {
            let meta = CodexStatusMeta {
                model: resolved_model.display_model.clone(),
                reasoning_effort: resolved_model
                    .reasoning_effort
                    .clone()
                    .unwrap_or_else(|| "default".to_string()),
                project_path: project_path.display().to_string(),
                iteration: state.iteration,
                max_iterations: state.max_iterations,
                iteration_started_at: iteration_start,
            };
            if cli.codex_render == CodexRenderMode::Rich {
                render_codex_iteration_panel(&meta);
            }
            Some(meta)
        } else {
            None
        };

        if let (Some(tui), Some(meta)) = (codex_tui.as_mut(), codex_status_meta.as_ref()) {
            tui.set_meta(CodexTuiMeta {
                model: meta.model.clone(),
                reasoning_effort: meta.reasoning_effort.clone(),
                project_path: meta.project_path.clone(),
                iteration: meta.iteration,
                max_iterations: meta.max_iterations,
            })?;
        }

        // Build full prompt with context
        let mut full_prompt = state.prompt.clone();

        if let Some(context) = load_context()? {
            full_prompt = format!("{}\n\n{}", context, full_prompt);
        }

        if let Some(promise) = &state.promise {
            full_prompt = format!(
                "{}\n\nOutput <promise>{}</promise> when complete.",
                full_prompt, promise
            );
        }

        if let Some(tasks_file) = &state.tasks_file {
            full_prompt = format!(
                "{}\n\nMark all tasks complete in {}.",
                full_prompt, tasks_file
            );
        }

        let previous_ai_response = load_prev_ai_response()?;
        full_prompt = inject_prev_ai_context(&full_prompt, previous_ai_response.as_deref());
        if agent_type == AgentType::Codex {
            full_prompt = prepend_codex_file_edit_prompt(&full_prompt);
        }

        // Execute agent
        let agent = create_default_agent(agent_type);

        let output_last_message_path =
            (agent_type == AgentType::Codex).then_some(get_last_message_capture_path());

        let effective_codex_resume_session = if use_codex_resume {
            state.codex_resume_session.clone().or_else(|| {
                if state.iteration == 1 {
                    cli.codex_resume.clone()
                } else {
                    None
                }
            })
        } else {
            None
        };
        let effective_codex_resume_last =
            use_codex_resume && effective_codex_resume_session.is_none() && cli.codex_resume_last;

        let args_options = AgentBuildArgsOptions {
            allow_all_permissions: false,
            codex_resume_last: effective_codex_resume_last,
            codex_resume_session: effective_codex_resume_session,
            extra_flags: Vec::new(),
            stream_output: true,
            sandbox_mode: (agent_type == AgentType::Codex).then_some(cli.codex_sandbox),
            approval_policy: (agent_type == AgentType::Codex).then_some(cli.codex_approval),
            extra_writable_dirs: if agent_type == AgentType::Codex {
                cli.codex_add_dirs.clone()
            } else {
                Vec::new()
            },
            output_last_message_path: output_last_message_path.clone(),
        };

        let env_options = AgentEnvOptions {
            filter_plugins: false,
            allow_all_permissions: false,
        };

        let args = agent.build_args(&full_prompt, &resolved_model.execution_model, &args_options);
        let env = agent.build_env(&env_options);

        if output_last_message_path.is_some() {
            clear_last_message_capture()?;
        }

        let mut cmd = Command::new(agent.command());
        cmd.args(&args)
            .envs(&env)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        if agent_type == AgentType::Codex {
            cmd.stdin(Stdio::piped());
        }

        let mut child = cmd.spawn()?;

        if agent_type == AgentType::Codex {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| anyhow!("Failed to capture stdin"))?;
            stdin.write_all(full_prompt.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.shutdown().await?;
        }

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to capture stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("Failed to capture stderr"))?;

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut output_buffer = String::with_capacity(4096);
        let mut tools_used: HashMap<String, u32> = HashMap::new();
        let mut codex_json_processor =
            (agent_type == AgentType::Codex).then(CodexJsonEventProcessor::default);
        let mut codex_status_renderer = codex_status_meta.clone().map(CodexStatusRenderer::new);
        let mut codex_status_tick = interval(Duration::from_millis(120));
        codex_status_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        codex_status_tick.tick().await;
        let mut stdout_closed = false;
        let mut stderr_closed = false;

        // Read output streams
        while !stdout_closed || !stderr_closed {
            if let Some(tui) = codex_tui.as_mut() {
                if matches!(tui.handle_input()?, TuiInputAction::ExitRequested) {
                    let _ = child.kill().await;
                    if let Some(tui) = codex_tui.take() {
                        tui.finish()?;
                    }
                    println!();
                    println!("{}", "Interrupted by user".yellow().bold());
                    println!("{}", "State preserved; use --resume to continue.".dimmed());
                    return Ok(());
                }
            }

            tokio::select! {
                biased;
                line = stdout_reader.next_line(), if !stdout_closed => {
                    match line {
                        Ok(Some(line)) => {
                            if agent_type == AgentType::Codex
                                && is_transient_codex_progress_line(&line)
                            {
                                if let (Some(tui), Some(renderer)) =
                                    (codex_tui.as_mut(), codex_status_renderer.as_ref())
                                {
                                    tui.set_runtime(
                                        renderer.current_snapshot(),
                                        renderer.current_status_line(),
                                    )?;
                                }
                                continue;
                            }

                            if let Some(processor) = codex_json_processor.as_mut() {
                                match processor.process_line(&line) {
                                    Ok(batch) => {
                                        let progress = processor.current_progress();
                                        let lines = batch.lines;
                                        let output_buffer_text = batch.output_buffer_text;
                                        let tool_name = batch.tool_name;

                                        match cli.codex_render {
                                            CodexRenderMode::Tui => {
                                                if let Some(renderer) = codex_status_renderer.as_mut() {
                                                    renderer.note_output_activity();
                                                }
                                                if let Some(tui) = codex_tui.as_mut() {
                                                    for render_line in lines {
                                                        tui.push_render_line(render_line)?;
                                                    }
                                                }
                                            }
                                            CodexRenderMode::JsonPass => {
                                                if let Some(renderer) = codex_status_renderer.as_mut() {
                                                    renderer.note_output_activity();
                                                }
                                                println!("{}", line);
                                            }
                                            CodexRenderMode::EventJson => {
                                                if let Some(renderer) = codex_status_renderer.as_mut() {
                                                    renderer.note_output_activity();
                                                }
                                                print_codex_ui_event(CodexUiEvent::from_batch(
                                                    &ralph_wiggum_rs::CodexRenderBatch {
                                                        lines: lines.clone(),
                                                        output_buffer_text: output_buffer_text.clone(),
                                                        tool_name: tool_name.clone(),
                                                    },
                                                    progress.clone(),
                                                ));
                                            }
                                            CodexRenderMode::Plain | CodexRenderMode::Rich => {
                                                for render_line in lines {
                                                    if let Some(renderer) = codex_status_renderer.as_mut() {
                                                        renderer.note_output_activity();
                                                    }
                                                    print_codex_render_line(render_line);
                                                }
                                            }
                                        }

                                        if let Some(text) = output_buffer_text {
                                            output_buffer.push_str(&text);
                                            output_buffer.push('\n');
                                        }

                                        if let Some(tool) = tool_name {
                                            *tools_used.entry(tool).or_insert(0) += 1;
                                        }

                                        if let Some(renderer) = codex_status_renderer.as_mut() {
                                            renderer.update(&progress);
                                            if let Some(tui) = codex_tui.as_mut() {
                                                tui.set_runtime(
                                                    Some(progress.clone()),
                                                    renderer.current_status_line(),
                                                )?;
                                            }
                                        }
                                        if state.one_session && agent_type == AgentType::Codex {
                                            if let Some(thread_id) = progress.thread_id.clone() {
                                                if state.codex_resume_session.as_deref()
                                                    != Some(thread_id.as_str())
                                                {
                                                    state.codex_resume_session = Some(thread_id);
                                                    save_state(&state)?;
                                                }
                                            }
                                        }
                                    }
                                    Err(_) => {
                                        if let Some(renderer) = codex_status_renderer.as_mut() {
                                            renderer.note_output_activity();
                                        }
                                        match cli.codex_render {
                                            CodexRenderMode::Tui => {
                                                if let Some(tui) = codex_tui.as_mut() {
                                                    tui.push_raw_stdout(line.clone())?;
                                                    if let Some(renderer) = codex_status_renderer.as_ref() {
                                                        tui.set_runtime(
                                                            renderer.current_snapshot(),
                                                            renderer.current_status_line(),
                                                        )?;
                                                    }
                                                }
                                            }
                                            CodexRenderMode::EventJson => {
                                                print_codex_ui_event(CodexUiEvent::parse_error(line.clone()));
                                            }
                                            _ => {
                                                println!("{}", line.dimmed());
                                            }
                                        }
                                    }
                                }
                            } else {
                                if let Some(renderer) = codex_status_renderer.as_mut() {
                                    renderer.note_output_activity();
                                }
                                if let Some(tui) = codex_tui.as_mut() {
                                    tui.push_raw_stdout(line.clone())?;
                                    if let Some(renderer) = codex_status_renderer.as_ref() {
                                        tui.set_runtime(
                                            renderer.current_snapshot(),
                                            renderer.current_status_line(),
                                        )?;
                                    }
                                } else {
                                    println!("{}", line);
                                }
                                output_buffer.push_str(&line);
                                output_buffer.push('\n');

                                if let Some(tool) = agent.parse_tool_output(&line) {
                                    *tools_used.entry(tool).or_insert(0) += 1;
                                }
                            }
                        }
                        Ok(None) => stdout_closed = true,
                        Err(e) => {
                            if let Some(renderer) = codex_status_renderer.as_mut() {
                                renderer.note_output_activity();
                            }
                            if let Some(tui) = codex_tui.as_mut() {
                                tui.push_stderr(format!("Error reading stdout: {}", e))?;
                            } else {
                                eprintln!("{}", format!("Error reading stdout: {}", e).red());
                            }
                            stdout_closed = true;
                        }
                    }
                }
                line = stderr_reader.next_line(), if !stderr_closed => {
                    match line {
                        Ok(Some(line)) => {
                            let handled_by_status = codex_status_renderer
                                .as_mut()
                                .map(|renderer| renderer.observe_stderr(&line))
                                .unwrap_or(false);

                            if !handled_by_status {
                                if let Some(renderer) = codex_status_renderer.as_mut() {
                                    renderer.note_output_activity();
                                }
                                if agent_type == AgentType::Codex && cli.codex_render == CodexRenderMode::EventJson {
                                    print_codex_ui_event(CodexUiEvent::stderr(line.clone()));
                                } else if let Some(tui) = codex_tui.as_mut() {
                                    tui.push_stderr(line.clone())?;
                                } else {
                                    eprintln!("{}", line.dimmed());
                                }
                            }

                            if let (Some(tui), Some(renderer)) =
                                (codex_tui.as_mut(), codex_status_renderer.as_ref())
                            {
                                tui.set_runtime(
                                    renderer.current_snapshot(),
                                    renderer.current_status_line(),
                                )?;
                            }
                        }
                        Ok(None) => stderr_closed = true,
                        Err(e) => {
                            if let Some(renderer) = codex_status_renderer.as_mut() {
                                renderer.note_output_activity();
                            }
                            if let Some(tui) = codex_tui.as_mut() {
                                tui.push_stderr(format!("Error reading stderr: {}", e))?;
                            } else {
                                eprintln!("{}", format!("Error reading stderr: {}", e).red());
                            }
                            stderr_closed = true;
                        }
                    }
                }
                _ = codex_status_tick.tick(), if codex_status_renderer.is_some() => {
                    if let Some(renderer) = codex_status_renderer.as_mut() {
                        if let Some(tui) = codex_tui.as_mut() {
                            tui.set_runtime(
                                renderer.current_snapshot(),
                                renderer.current_status_line(),
                            )?;
                        } else {
                            renderer.tick();
                        }
                    }
                }
            }
        }

        if let Some(renderer) = codex_status_renderer.as_mut() {
            renderer.finish();
        }
        if let Some(tui) = codex_tui.as_mut() {
            tui.set_runtime(None, None)?;
        }

        let exit_status = child.wait().await?;
        let exit_code = exit_status.code().unwrap_or(-1);

        let latest_ai_response = load_last_message_capture()?
            .map(|content| content.trim().to_string())
            .filter(|content| !content.is_empty())
            .or_else(|| {
                if exit_code == 0 {
                    let fallback = output_buffer.trim();
                    (!fallback.is_empty()).then(|| fallback.to_string())
                } else {
                    None
                }
            });

        if let Some(latest_ai_response) = latest_ai_response.as_ref() {
            save_prev_ai_response(latest_ai_response)?;
        }

        let mut completion_output = output_buffer.clone();
        if let Some(latest_ai_response) = latest_ai_response.as_ref() {
            let latest_ai_response = latest_ai_response.trim();
            if !latest_ai_response.is_empty() && !completion_output.contains(latest_ai_response) {
                if !completion_output.is_empty() {
                    completion_output.push('\n');
                }
                completion_output.push_str(latest_ai_response);
            }
        }

        let iteration_duration = iteration_start.elapsed().as_millis() as u64;

        // Check completion
        let mut completion_detected = false;

        if let Some(promise) = &state.promise {
            if check_terminal_promise(&completion_output, promise) {
                if let Some(tui) = codex_tui.as_mut() {
                    tui.push_raw_stdout("Promise detected!")?;
                } else {
                    println!("{}", "Promise detected!".green().bold());
                }
                completion_detected = true;
            }
        }

        if let Some(_) = &state.tasks_file {
            if let Ok(Some(tasks_content)) = load_tasks() {
                if tasks_markdown_all_complete(&tasks_content) {
                    if let Some(tui) = codex_tui.as_mut() {
                        tui.push_raw_stdout("All tasks complete!")?;
                    } else {
                        println!("{}", "All tasks complete!".green().bold());
                    }
                    completion_detected = true;
                }
            }
        }

        // Record iteration
        let iteration_record = IterationHistory {
            iteration: state.iteration,
            started_at: chrono::Utc::now().to_rfc3339(),
            ended_at: chrono::Utc::now().to_rfc3339(),
            duration_ms: iteration_duration,
            agent: current_agent.clone(),
            model: resolved_model.display_model.clone(),
            tools_used,
            files_modified: Vec::new(),
            exit_code,
            completion_detected,
            errors: if exit_code != 0 {
                vec![format!("Exit code: {}", exit_code)]
            } else {
                Vec::new()
            },
        };

        history.iterations.push(iteration_record);
        history.total_duration_ms += iteration_duration;
        save_history(&history)?;

        if completion_detected {
            if let Some(tui) = codex_tui.as_mut() {
                tui.push_raw_stdout("Task completed successfully!")?;
                tui.push_raw_stdout(format!("Total iterations: {}", state.iteration))?;
                tui.push_raw_stdout(format!("Total time: {}ms", history.total_duration_ms))?;
            }
            if let Some(tui) = codex_tui.take() {
                tui.finish()?;
            }
            println!();
            println!("{}", "Task completed successfully!".green().bold());
            println!(
                "{}",
                format!("   Total iterations: {}", state.iteration).dimmed()
            );
            println!(
                "{}",
                format!("   Total time: {}ms", history.total_duration_ms).dimmed()
            );
            clear_state()?;
            return Ok(());
        }

        // Rotate agent if needed
        if let Some(rotation) = &state.rotation {
            if !rotation.is_empty() {
                state.rotation_index =
                    Some((state.rotation_index.unwrap_or(0) + 1) % rotation.len());
            }
        }

        state.iteration += 1;
        save_state(&state)?;

        if state.iteration <= state.max_iterations {
            if let Some(tui) = codex_tui.as_mut() {
                tui.push_raw_stdout(format!("Waiting {}s before next iteration...", cli.delay))?;
                tui.set_footer(Some(format!(
                    "Waiting {}s before next iteration...",
                    cli.delay
                )))?;
            } else {
                println!();
                println!(
                    "{}",
                    format!("Waiting {}s before next iteration...", cli.delay).dimmed()
                );
            }
            if codex_tui.is_some() {
                let wait_start = Instant::now();
                while wait_start.elapsed() < Duration::from_secs(cli.delay) {
                    sleep(Duration::from_millis(120)).await;
                    if let Some(tui) = codex_tui.as_mut() {
                        if matches!(tui.handle_input()?, TuiInputAction::ExitRequested) {
                            if let Some(tui) = codex_tui.take() {
                                tui.finish()?;
                            }
                            println!();
                            println!("{}", "Interrupted by user".yellow().bold());
                            println!("{}", "State preserved; use --resume to continue.".dimmed());
                            return Ok(());
                        }
                    }
                }
            } else {
                sleep(Duration::from_secs(cli.delay)).await;
            }
            if let Some(tui) = codex_tui.as_mut() {
                tui.set_footer(None)?;
            } else {
                println!();
            }
        }
    }

    if let Some(mut tui) = codex_tui.take() {
        tui.push_raw_stdout("Maximum iterations reached")?;
        tui.push_raw_stdout(format!("Total time: {}ms", history.total_duration_ms))?;
        tui.finish()?;
    }
    println!();
    println!("{}", "Maximum iterations reached".yellow().bold());
    println!(
        "{}",
        format!("   Total time: {}ms", history.total_duration_ms).dimmed()
    );
    clear_state()?;

    Ok(())
}

#[derive(Debug)]
struct CodexStatusRenderer {
    meta: CodexStatusMeta,
    snapshot: Option<CodexProgressSnapshot>,
    mcp_startup: McpStartupTracker,
    is_visible: bool,
    last_output_at: Instant,
}

impl CodexStatusRenderer {
    fn new(meta: CodexStatusMeta) -> Self {
        Self {
            meta,
            snapshot: None,
            mcp_startup: McpStartupTracker::default(),
            is_visible: false,
            last_output_at: Instant::now(),
        }
    }

    fn clear_for_log_line(&mut self) {
        if self.is_visible {
            eprint!("\r\x1b[2K");
            let _ = std::io::stderr().flush();
            self.is_visible = false;
        }
    }

    fn note_output_activity(&mut self) {
        self.clear_for_log_line();
        self.last_output_at = Instant::now();
    }

    fn update(&mut self, snapshot: &CodexProgressSnapshot) {
        self.snapshot = Some(snapshot.clone());
    }

    fn observe_stderr(&mut self, line: &str) -> bool {
        self.mcp_startup.observe_line(line)
    }

    fn tick(&mut self) {
        self.redraw();
    }

    fn current_status_line(&self) -> Option<String> {
        if self.last_output_at.elapsed() < Duration::from_millis(450) {
            return None;
        }

        if let Some(header) = self.mcp_startup.header() {
            return Some(build_codex_status_line_with_header(
                &self.meta,
                self.snapshot.as_ref(),
                &header,
            ));
        }

        let snapshot = self.snapshot.as_ref()?;
        if !codex_snapshot_should_show_footer(snapshot) {
            return None;
        }

        Some(build_codex_status_line(&self.meta, snapshot))
    }

    fn current_snapshot(&self) -> Option<CodexProgressSnapshot> {
        self.snapshot.clone()
    }

    fn redraw(&mut self) {
        let Some(status) = self.current_status_line() else {
            self.clear_for_log_line();
            return;
        };
        eprint!("\r\x1b[2K{}", status.dimmed());
        let _ = std::io::stderr().flush();
        self.is_visible = true;
    }

    fn finish(&mut self) {
        self.clear_for_log_line();
        self.snapshot = None;
    }
}

fn print_codex_ui_event(event: CodexUiEvent) {
    match serde_json::to_string(&event) {
        Ok(line) => println!("{}", line),
        Err(error) => {
            let fallback = format!(
                "{{\"type\":\"serialization_error\",\"message\":{:?}}}",
                error.to_string()
            );
            println!("{}", fallback);
        }
    }
}

fn print_codex_render_line(render_line: CodexRenderLine) {
    match render_line.kind {
        CodexRenderLineKind::Assistant => println!("{}", render_line.text),
        CodexRenderLineKind::Reasoning => println!("{}", render_line.text.dimmed()),
        CodexRenderLineKind::Tool => println!("{}", render_line.text.cyan()),
        CodexRenderLineKind::ToolOutput => println!("{}", render_line.text.dimmed()),
        CodexRenderLineKind::Status => println!("{}", render_line.text.dimmed()),
        CodexRenderLineKind::Error => eprintln!("{}", render_line.text.red()),
        CodexRenderLineKind::Todo => println!("{}", render_line.text.yellow()),
    }
}

fn show_status() -> Result<()> {
    if !state_exists() {
        println!("{}", "No active Ralph loop".dimmed());
        return Ok(());
    }

    let state = load_state()?;
    let history = load_history()?;

    println!("{}", "ACTIVE LOOP".cyan().bold());
    println!(
        "{}",
        format!(
            "   Iteration:    {} / {}",
            state.iteration, state.max_iterations
        )
    );
    println!("{}", format!("   Prompt:       {}", state.prompt));

    if let Some(rotation) = &state.rotation {
        println!();
        println!(
            "{}",
            format!(
                "   Rotation (position {}/{}):",
                state.rotation_index.unwrap_or(0) + 1,
                rotation.len()
            )
        );
        for (i, pair) in rotation.iter().enumerate() {
            let agent_type = AgentType::from_str(&pair.agent)?;
            let resolved_model = resolve_model_selection(
                agent_type,
                Some(&pair.model),
                &CodexConfigDefaults::default(),
            );
            let marker = if Some(i) == state.rotation_index {
                "**ACTIVE**"
            } else {
                ""
            };
            println!(
                "   {}. {}:{}  {}",
                i + 1,
                pair.agent,
                resolved_model.display_model,
                marker
            );
        }
    }

    if !history.iterations.is_empty() {
        println!();
        println!(
            "{}",
            format!("HISTORY ({} iterations)", history.iterations.len())
                .cyan()
                .bold()
        );
        println!(
            "{}",
            format!("   Total time:   {}ms", history.total_duration_ms)
        );
        println!();
        println!("{}", "   Recent iterations:".dimmed());

        for iter in history.iterations.iter().rev().take(5) {
            let tools_summary: String = iter
                .tools_used
                .iter()
                .map(|(name, count)| format!("{}({})", name, count))
                .collect::<Vec<_>>()
                .join(" ");

            println!(
                "   #{}  {}ms  {} / {}  {}",
                iter.iteration, iter.duration_ms, iter.agent, iter.model, tools_summary
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_model_selection_uses_codex_config_default_when_unset() {
        let resolved =
            resolve_model_selection(AgentType::Codex, None, &CodexConfigDefaults::default());

        assert!(resolved.execution_model.is_empty());
        assert_eq!(resolved.display_model, "codex-config-default");
    }

    #[test]
    fn resolve_model_selection_uses_agent_default_when_available() {
        let resolved =
            resolve_model_selection(AgentType::Opencode, None, &CodexConfigDefaults::default());

        assert_eq!(resolved.execution_model, "claude-sonnet-4");
        assert_eq!(resolved.display_model, "claude-sonnet-4");
    }

    #[test]
    fn resolve_model_selection_reads_codex_model_from_config_defaults() {
        let resolved = resolve_model_selection(
            AgentType::Codex,
            None,
            &CodexConfigDefaults {
                model: Some("gpt-5.3-codex".to_string()),
                reasoning_effort: Some("high".to_string()),
            },
        );

        assert!(resolved.execution_model.is_empty());
        assert_eq!(resolved.display_model, "gpt-5.3-codex");
        assert_eq!(resolved.reasoning_effort, Some("high".to_string()));
    }

    #[test]
    fn parse_top_level_toml_string_ignores_nested_sections() {
        let contents = r#"
model = "gpt-5.4"
model_reasoning_effort = "xhigh"

[model_providers.custom]
model = "nested-value"
"#;

        assert_eq!(
            parse_top_level_toml_string(contents, "model"),
            Some("gpt-5.4".to_string())
        );
        assert_eq!(
            parse_top_level_toml_string(contents, "model_reasoning_effort"),
            Some("xhigh".to_string())
        );
    }

    #[test]
    fn build_codex_status_line_includes_tokens_loop_and_path() {
        let meta = CodexStatusMeta {
            model: "gpt-5.4".to_string(),
            reasoning_effort: "xhigh".to_string(),
            project_path: "D:/Dev-Project/demo".to_string(),
            iteration: 2,
            max_iterations: 5,
            iteration_started_at: Instant::now(),
        };
        let snapshot = CodexProgressSnapshot {
            status_line: "Codex thinking | tools 2".to_string(),
            phase: "thinking".to_string(),
            status_header: Some("Testing and patching code".to_string()),
            thread_id: Some("thread-1".to_string()),
            todo_completed: 0,
            todo_total: 0,
            tool_calls: 2,
            last_tool: Some("shell".to_string()),
            last_detail: Some("ls".to_string()),
            last_error: None,
            input_tokens: Some(12_300),
            cached_input_tokens: Some(4_000),
            output_tokens: Some(512),
        };

        let line = build_codex_status_line(&meta, &snapshot);

        assert!(line.contains("Testing and patching code"));
        assert!(line.contains("ctrl+c to interrupt"));
        assert!(line.contains("loop 2/5"));
        assert!(line.contains("model gpt-5.4"));
        assert!(line.contains("effort xhigh"));
        assert!(line.contains("tok in 12.3k"));
        assert!(line.contains("path"));
    }

    #[test]
    fn mcp_startup_tracker_formats_native_like_header() {
        let mut tracker = McpStartupTracker::default();

        assert!(tracker.observe_line("mcp: filesystem starting"));
        assert!(tracker.observe_line("mcp: context7 starting"));
        assert!(tracker.observe_line("mcp: npm-search ready"));

        assert_eq!(
            tracker.header(),
            Some("Starting MCP servers (1/3): context7, filesystem".to_string())
        );
    }

    #[test]
    fn mcp_startup_tracker_uses_single_server_boot_message() {
        let mut tracker = McpStartupTracker::default();

        assert!(tracker.observe_line("mcp: filesystem starting"));

        assert_eq!(
            tracker.header(),
            Some("Booting MCP server: filesystem".to_string())
        );
    }

    #[test]
    fn codex_snapshot_footer_hidden_for_responding_phase() {
        let snapshot = CodexProgressSnapshot {
            status_line: "Codex responding".to_string(),
            phase: "responding".to_string(),
            status_header: Some("Writing response".to_string()),
            thread_id: Some("thread-1".to_string()),
            todo_completed: 0,
            todo_total: 0,
            tool_calls: 1,
            last_tool: Some("shell".to_string()),
            last_detail: Some("assistant progress note".to_string()),
            last_error: None,
            input_tokens: None,
            cached_input_tokens: None,
            output_tokens: None,
        };

        assert!(!codex_snapshot_should_show_footer(&snapshot));
    }

    #[test]
    fn prepend_codex_file_edit_prompt_adds_apply_patch_guidance() {
        let prompt = prepend_codex_file_edit_prompt("finish the feature");

        assert!(prompt.contains(CODEX_FILE_EDIT_SYSTEM_PROMPT));
        assert!(prompt.ends_with("finish the feature"));
    }

    #[test]
    fn transient_codex_progress_lines_are_detected() {
        assert!(is_transient_codex_progress_line(
            "◐ Writing response (17s • ctrl+c to interrupt) · model gpt-5.3-codex"
        ));
        assert!(is_transient_codex_progress_line(
            "• Testing and patching code (1m 52s • esc to interrupt)"
        ));
        assert!(!is_transient_codex_progress_line(
            "[tool:shell:completed] powershell -Command 'Get-ChildItem'"
        ));
    }
}

fn stop_loop() -> Result<()> {
    if !state_exists() {
        println!("{}", "No active Ralph loop to stop".dimmed());
        return Ok(());
    }

    clear_state()?;
    println!("{}", "Stopped Ralph loop".green());
    Ok(())
}
