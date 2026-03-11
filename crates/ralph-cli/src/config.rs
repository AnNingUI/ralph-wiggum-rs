//! Model resolution and Codex config loading.

use std::env;
use std::path::PathBuf;

use ralph_core::types::AgentType;

/// Resolved model selection for an iteration.
#[derive(Debug, Clone)]
pub struct ResolvedModel {
    /// Model name passed to the agent CLI (empty = use agent default).
    pub execution_model: String,
    /// Human-readable model name for status display.
    pub display_model: String,
    /// Reasoning effort level (Codex-specific).
    pub reasoning_effort: Option<String>,
}

/// Codex config.toml defaults (model, reasoning_effort).
#[derive(Debug, Clone, Default)]
pub struct CodexConfigDefaults {
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
}

/// Resolve the model to use for a given agent type.
///
/// Priority: explicit CLI model > agent default > codex config > implicit label
pub fn resolve_model(
    agent_type: AgentType,
    requested_model: Option<&str>,
    codex_config: &CodexConfigDefaults,
) -> ResolvedModel {
    let requested = requested_model
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(String::from);

    // Explicit model from CLI
    if let Some(explicit) = requested {
        let effort = if uses_codex_config(agent_type) {
            codex_config.reasoning_effort.clone()
        } else {
            None
        };
        return ResolvedModel {
            execution_model: explicit.clone(),
            display_model: explicit,
            reasoning_effort: effort,
        };
    }

    // Agent built-in default
    if let Some(default) = agent_type.default_model() {
        return ResolvedModel {
            execution_model: default.to_string(),
            display_model: default.to_string(),
            reasoning_effort: None,
        };
    }

    // Codex config.toml default
    if uses_codex_config(agent_type) {
        if let Some(configured) = codex_config.model.clone() {
            return ResolvedModel {
                execution_model: String::new(),
                display_model: configured,
                reasoning_effort: codex_config.reasoning_effort.clone(),
            };
        }
    }

    // Fallback: implicit label
    ResolvedModel {
        execution_model: String::new(),
        display_model: agent_type.implicit_model_label().to_string(),
        reasoning_effort: None,
    }
}

fn uses_codex_config(agent_type: AgentType) -> bool {
    matches!(agent_type, AgentType::Codex)
}

/// Load Codex config defaults from `$CODEX_HOME/config.toml` or `~/.codex/config.toml`.
/// This is only used for display purposes (showing the model name in status).
/// The actual model selection is handled by Codex itself when --model is not specified.
pub fn load_codex_config_defaults() -> CodexConfigDefaults {
    let Some(codex_home) = find_codex_home() else {
        return CodexConfigDefaults::default();
    };

    let config_path = codex_home.join("config.toml");
    let Ok(contents) = std::fs::read_to_string(config_path) else {
        return CodexConfigDefaults::default();
    };

    CodexConfigDefaults {
        model: parse_toml_string(&contents, "model"),
        reasoning_effort: parse_toml_string(&contents, "model_reasoning_effort"),
    }
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
        .map(|p| p.join(".codex"))
}

/// Parse a top-level `key = "value"` from a TOML file (ignoring nested sections).
pub fn parse_toml_string(contents: &str, key: &str) -> Option<String> {
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

        let value = candidate_value.split('#').next().unwrap_or_default().trim();
        if value.is_empty() {
            return None;
        }

        // Strip surrounding quotes
        if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
            return Some(value[1..value.len() - 1].to_string());
        }

        return Some(value.to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_model_uses_explicit_over_default() {
        let resolved = resolve_model(
            AgentType::ClaudeCode,
            Some("claude-opus-4"),
            &CodexConfigDefaults::default(),
        );
        assert_eq!(resolved.execution_model, "claude-opus-4");
        assert_eq!(resolved.display_model, "claude-opus-4");
    }

    #[test]
    fn resolve_model_falls_back_to_agent_default() {
        let resolved = resolve_model(
            AgentType::ClaudeCode,
            None,
            &CodexConfigDefaults::default(),
        );
        assert_eq!(resolved.execution_model, "claude-sonnet-4");
        assert_eq!(resolved.display_model, "claude-sonnet-4");
    }

    #[test]
    fn resolve_model_uses_codex_config_for_codex() {
        let config = CodexConfigDefaults {
            model: Some("gpt-5.4".into()),
            reasoning_effort: Some("xhigh".into()),
        };
        let resolved = resolve_model(AgentType::Codex, None, &config);
        assert!(resolved.execution_model.is_empty());
        assert_eq!(resolved.display_model, "gpt-5.4");
        assert_eq!(resolved.reasoning_effort.as_deref(), Some("xhigh"));
    }

    #[test]
    fn parse_toml_string_ignores_nested_sections() {
        let contents = r#"
model = "gpt-5.4"
model_reasoning_effort = "xhigh"

[model_providers.custom]
model = "nested-value"
"#;
        assert_eq!(
            parse_toml_string(contents, "model"),
            Some("gpt-5.4".to_string())
        );
        assert_eq!(
            parse_toml_string(contents, "model_reasoning_effort"),
            Some("xhigh".to_string())
        );
    }
}
