//! CLI validation rules for agent options.
//!
//! Validates resume/fork/one-session logic before execution.

use anyhow::{Result, anyhow};

use ralph_core::options::AgentOptions;
use ralph_core::types::AgentType;

/// Validate Codex resume options.
///
/// Rules:
/// - --codex-resume* can only be used on iteration 1
/// - --one-session requires Codex on iteration 1
pub fn validate_codex_resume(
    agent_type: AgentType,
    options: &AgentOptions,
    iteration: u32,
    one_session: bool,
) -> Result<()> {
    let wants_resume = options.codex.resume_last || options.codex.resume_session.is_some();

    if wants_resume && iteration != 1 {
        return Err(anyhow!(
            "--codex-resume* can only be used when starting a fresh loop (iteration 1) for agent {}",
            agent_type.as_str()
        ));
    }

    if iteration == 1 && one_session && !matches!(agent_type, AgentType::Codex) {
        return Err(anyhow!(
            "--one-session requires --agent codex on the first iteration"
        ));
    }

    Ok(())
}

/// Validate that non-Codex agents don't use Codex-specific options on iteration 1.
pub fn validate_non_codex_first_iteration(
    agent_type: AgentType,
    options: &AgentOptions,
    iteration: u32,
    one_session: bool,
) -> Result<()> {
    if matches!(agent_type, AgentType::Codex) {
        return Ok(());
    }

    let wants_resume = options.codex.resume_last || options.codex.resume_session.is_some();

    if iteration == 1 && (one_session || wants_resume) {
        return Err(anyhow!(
            "--codex-resume* and --one-session require --agent codex on the first iteration"
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ralph_core::options::CodexOptions;

    #[test]
    fn codex_resume_allowed_on_iteration_1() {
        let options = AgentOptions {
            codex: CodexOptions {
                resume_last: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(validate_codex_resume(AgentType::Codex, &options, 1, false).is_ok());
    }

    #[test]
    fn codex_resume_rejected_on_iteration_2() {
        let options = AgentOptions {
            codex: CodexOptions {
                resume_last: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(validate_codex_resume(AgentType::Codex, &options, 2, false).is_err());
    }

    #[test]
    fn one_session_requires_codex_on_iteration_1() {
        let options = AgentOptions::default();
        assert!(validate_non_codex_first_iteration(
            AgentType::ClaudeCode,
            &options,
            1,
            true
        )
        .is_err());
    }

    #[test]
    fn one_session_allowed_for_codex() {
        let options = AgentOptions::default();
        assert!(validate_codex_resume(AgentType::Codex, &options, 1, true).is_ok());
    }
}
