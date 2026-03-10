//! Agent configuration and management

use anyhow::{Result, anyhow};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::completion::strip_ansi;

static OPENCODE_PATTERN: OnceLock<Regex> = OnceLock::new();
static CLAUDE_PATTERN1: OnceLock<Regex> = OnceLock::new();
static CLAUDE_PATTERN2: OnceLock<Regex> = OnceLock::new();

fn get_opencode_pattern() -> &'static Regex {
    OPENCODE_PATTERN.get_or_init(|| Regex::new(r"^\|\s{2}([A-Za-z0-9_-]+)").unwrap())
}

fn get_claude_pattern1() -> &'static Regex {
    CLAUDE_PATTERN1
        .get_or_init(|| Regex::new(r"(?:Using|Called|Tool:)\s+([A-Za-z0-9_.-]+)").unwrap())
}

fn get_claude_pattern2() -> &'static Regex {
    CLAUDE_PATTERN2.get_or_init(|| Regex::new(r#""name"\s*:\s*"([^"]+)""#).unwrap())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentType {
    Opencode,
    ClaudeCode,
    Codex,
    Copilot,
}

impl AgentType {
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "opencode" => Ok(AgentType::Opencode),
            "claude-code" => Ok(AgentType::ClaudeCode),
            "codex" => Ok(AgentType::Codex),
            "copilot" => Ok(AgentType::Copilot),
            _ => Err(anyhow!("Unknown agent type: {}", s)),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            AgentType::Opencode => "opencode",
            AgentType::ClaudeCode => "claude-code",
            AgentType::Codex => "codex",
            AgentType::Copilot => "copilot",
        }
    }

    pub fn default_model(&self) -> Option<&'static str> {
        match self {
            AgentType::Opencode | AgentType::ClaudeCode => Some("claude-sonnet-4"),
            AgentType::Codex | AgentType::Copilot => None,
        }
    }

    pub fn implicit_model_label(&self) -> &'static str {
        match self {
            AgentType::Opencode | AgentType::ClaudeCode => "default",
            AgentType::Codex => "codex-config-default",
            AgentType::Copilot => "copilot-default",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxMode {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

impl SandboxMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SandboxMode::ReadOnly => "read-only",
            SandboxMode::WorkspaceWrite => "workspace-write",
            SandboxMode::DangerFullAccess => "danger-full-access",
        }
    }
}

impl fmt::Display for SandboxMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalPolicy {
    Untrusted,
    OnFailure,
    OnRequest,
    Never,
}

impl ApprovalPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApprovalPolicy::Untrusted => "untrusted",
            ApprovalPolicy::OnFailure => "on-failure",
            ApprovalPolicy::OnRequest => "on-request",
            ApprovalPolicy::Never => "never",
        }
    }
}

impl fmt::Display for ApprovalPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct AgentEnvOptions {
    pub filter_plugins: bool,
    pub allow_all_permissions: bool,
}

#[derive(Debug, Clone)]
pub struct AgentBuildArgsOptions {
    pub allow_all_permissions: bool,
    pub codex_resume_last: bool,
    pub codex_resume_session: Option<String>,
    pub extra_flags: Vec<String>,
    pub stream_output: bool,
    pub sandbox_mode: Option<SandboxMode>,
    pub approval_policy: Option<ApprovalPolicy>,
    pub extra_writable_dirs: Vec<PathBuf>,
    pub output_last_message_path: Option<PathBuf>,
}

pub trait AgentConfig {
    fn agent_type(&self) -> AgentType;
    fn command(&self) -> &str;
    fn config_name(&self) -> &str;
    fn build_args(&self, prompt: &str, model: &str, options: &AgentBuildArgsOptions)
    -> Vec<String>;
    fn build_env(&self, options: &AgentEnvOptions) -> HashMap<String, String>;
    fn parse_tool_output(&self, line: &str) -> Option<String>;
}

#[derive(Debug, Clone)]
pub struct DefaultAgentConfig {
    pub agent_type: AgentType,
    pub command: String,
    pub config_name: String,
}

impl AgentConfig for DefaultAgentConfig {
    fn agent_type(&self) -> AgentType {
        self.agent_type
    }

    fn command(&self) -> &str {
        &self.command
    }

    fn config_name(&self) -> &str {
        &self.config_name
    }

    fn build_args(
        &self,
        prompt: &str,
        model: &str,
        options: &AgentBuildArgsOptions,
    ) -> Vec<String> {
        let mut args = Vec::with_capacity(
            11 + options.extra_flags.len() + options.extra_writable_dirs.len() * 2,
        );

        match self.agent_type {
            AgentType::Opencode => {
                args.push(prompt.to_string());
                args.push("--model".to_string());
                args.push(model.to_string());
                if options.stream_output {
                    args.push("--stream".to_string());
                }
            }
            AgentType::ClaudeCode => {
                args.push(prompt.to_string());
                args.push("--model".to_string());
                args.push(model.to_string());
            }
            AgentType::Codex => {
                let _ = options.approval_policy;

                args.push("exec".to_string());

                if let Some(sandbox_mode) = options.sandbox_mode {
                    args.push("--sandbox".to_string());
                    args.push(sandbox_mode.to_string());
                }

                if !model.trim().is_empty() {
                    args.push("--model".to_string());
                    args.push(model.to_string());
                }

                for dir in &options.extra_writable_dirs {
                    args.push("--add-dir".to_string());
                    args.push(dir.display().to_string());
                }

                if let Some(output_last_message_path) = &options.output_last_message_path {
                    args.push("--output-last-message".to_string());
                    args.push(output_last_message_path.display().to_string());
                }

                args.push("--json".to_string());
                if options.codex_resume_last || options.codex_resume_session.is_some() {
                    args.push("resume".to_string());
                    if options.codex_resume_last {
                        args.push("--last".to_string());
                    }
                    if let Some(session) = &options.codex_resume_session {
                        args.push(session.clone());
                    }
                }

                let _ = prompt;
                args.push("-".to_string());
            }
            AgentType::Copilot => {
                args.push(prompt.to_string());
            }
        }

        args.extend_from_slice(&options.extra_flags);
        args
    }

    fn build_env(&self, options: &AgentEnvOptions) -> HashMap<String, String> {
        let mut env = HashMap::new();

        if options.filter_plugins {
            env.insert("FILTER_PLUGINS".to_string(), "true".to_string());
        }

        if options.allow_all_permissions {
            env.insert("ALLOW_ALL_PERMISSIONS".to_string(), "true".to_string());
        }

        env
    }

    fn parse_tool_output(&self, line: &str) -> Option<String> {
        match self.agent_type {
            AgentType::Opencode => {
                let clean_line = strip_ansi(line);
                get_opencode_pattern()
                    .captures(&clean_line)
                    .and_then(|cap| cap.get(1))
                    .map(|m| m.as_str().to_string())
            }
            AgentType::ClaudeCode => {
                let clean_line = strip_ansi(line);

                if let Some(cap) = get_claude_pattern1().captures(&clean_line) {
                    return cap.get(1).map(|m| m.as_str().to_string());
                }

                if clean_line.contains(r#""type":"tool_use"#)
                    || clean_line.contains(r#""type": "tool_use"#)
                {
                    if let Some(cap) = get_claude_pattern2().captures(&clean_line) {
                        return cap.get(1).map(|m| m.as_str().to_string());
                    }
                }

                None
            }
            AgentType::Codex | AgentType::Copilot => None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct JsonAgentConfig {
    #[serde(rename = "type")]
    pub agent_type: String,
    pub command: String,
    #[serde(rename = "configName")]
    pub config_name: String,
    #[serde(rename = "argsTemplate")]
    pub args_template: Option<String>,
    #[serde(rename = "envTemplate")]
    pub env_template: Option<String>,
    #[serde(rename = "parsePattern")]
    pub parse_pattern: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RalphConfig {
    pub version: String,
    pub agents: Vec<JsonAgentConfig>,
}

pub fn get_default_config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".config")
        .join("open-ralph-wiggum")
        .join("agents.json")
}

pub fn create_default_agent(agent_type: AgentType) -> Box<dyn AgentConfig> {
    let (command, config_name) = match agent_type {
        AgentType::Opencode => {
            #[cfg(target_os = "windows")]
            {
                ("opencode.cmd", "opencode")
            }
            #[cfg(not(target_os = "windows"))]
            {
                ("opencode", "opencode")
            }
        }
        AgentType::ClaudeCode => {
            #[cfg(target_os = "windows")]
            {
                ("claude-code.cmd", "claude-code")
            }
            #[cfg(not(target_os = "windows"))]
            {
                ("claude-code", "claude-code")
            }
        }
        AgentType::Codex => {
            #[cfg(target_os = "windows")]
            {
                ("codex.cmd", "codex")
            }
            #[cfg(not(target_os = "windows"))]
            {
                ("codex", "codex")
            }
        }
        AgentType::Copilot => {
            #[cfg(target_os = "windows")]
            {
                ("copilot.cmd", "copilot")
            }
            #[cfg(not(target_os = "windows"))]
            {
                ("copilot", "copilot")
            }
        }
    };

    Box::new(DefaultAgentConfig {
        agent_type,
        command: command.to_string(),
        config_name: config_name.to_string(),
    })
}
