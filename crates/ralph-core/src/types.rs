use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

#[cfg(feature = "clap")]
use clap::ValueEnum;

/// Supported agent types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(ValueEnum))]
pub enum AgentType {
    Opencode,
    ClaudeCode,
    Codex,
    Copilot,
}

impl FromStr for AgentType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "opencode" => Ok(Self::Opencode),
            "claude" => Ok(Self::ClaudeCode),
            "codex" => Ok(Self::Codex),
            "copilot" => Ok(Self::Copilot),
            _ => Err(anyhow::anyhow!("Unknown agent type: {s}")),
        }
    }
}

impl AgentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Opencode => "opencode",
            Self::ClaudeCode => "claude",
            Self::Codex => "codex",
            Self::Copilot => "copilot",
        }
    }

    pub fn default_model(&self) -> Option<&'static str> {
        None
    }

    pub fn implicit_model_label(&self) -> &'static str {
        match self {
            Self::Opencode => "default",
            Self::ClaudeCode => "default",
            Self::Codex => "default",
            Self::Copilot => "copilot-default",
        }
    }
}

impl fmt::Display for AgentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Sandbox permission level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(ValueEnum))]
pub enum SandboxMode {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

impl SandboxMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::WorkspaceWrite => "workspace-write",
            Self::DangerFullAccess => "danger-full-access",
        }
    }
}

impl fmt::Display for SandboxMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Approval policy for agent actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(ValueEnum))]
pub enum ApprovalPolicy {
    Untrusted,
    OnFailure,
    OnRequest,
    Never,
}

impl ApprovalPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Untrusted => "untrusted",
            Self::OnFailure => "on-failure",
            Self::OnRequest => "on-request",
            Self::Never => "never",
        }
    }
}

impl fmt::Display for ApprovalPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Claude output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(ValueEnum))]
pub enum ClaudeOutputFormat {
    Text,
    Json,
    StreamJson,
}

impl ClaudeOutputFormat {
    pub fn as_cli_value(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
            Self::StreamJson => "stream-json",
        }
    }

    pub fn is_stream_json(&self) -> bool {
        matches!(self, Self::StreamJson)
    }
}

/// Claude loop mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(ValueEnum))]
pub enum ClaudeLoopMode {
    Print,
    RalphPlugin,
}

impl ClaudeLoopMode {
    pub fn is_plugin(self) -> bool {
        matches!(self, Self::RalphPlugin)
    }
}

/// JSON config for loading agents from file.
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

/// Top-level ralph config file.
#[derive(Debug, Deserialize)]
pub struct RalphConfig {
    pub version: String,
    pub agents: Vec<JsonAgentConfig>,
}

/// Default config file location.
pub fn get_default_config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".config")
        .join("open-ralph-wiggum")
        .join("agents.json")
}
