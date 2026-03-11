use ralph_core::options::AgentOptions;

/// Build codex CLI arguments from AgentOptions.
pub fn build_codex_args(_prompt: &str, model: &str, options: &AgentOptions) -> Vec<String> {
    let codex = &options.codex;
    let common = &options.common;

    let mut args = Vec::with_capacity(16);

    args.push("exec".to_string());

    if let Some(sandbox_mode) = common.sandbox_mode {
        args.push("--sandbox".to_string());
        args.push(sandbox_mode.to_string());
    }

    if !model.trim().is_empty() {
        args.push("--model".to_string());
        args.push(model.to_string());
    }

    for dir in &common.extra_writable_dirs {
        args.push("--add-dir".to_string());
        args.push(dir.display().to_string());
    }

    if let Some(path) = &common.output_last_message_path {
        args.push("--output-last-message".to_string());
        args.push(path.display().to_string());
    }

    // Images
    for image in &codex.images {
        args.push("--image".to_string());
        args.push(image.display().to_string());
    }

    // Web search
    if codex.search {
        args.push("--search".to_string());
    }

    // Output schema
    if let Some(schema) = &codex.output_schema {
        args.push("--output-schema".to_string());
        args.push(schema.display().to_string());
    }

    args.push("--json".to_string());

    // Resume / Fork
    if codex.fork_last || codex.fork_session.is_some() {
        args.push("fork".to_string());
        if codex.fork_last {
            args.push("--last".to_string());
        }
        if let Some(session) = &codex.fork_session {
            args.push(session.clone());
        }
    } else if codex.resume_last || codex.resume_session.is_some() {
        args.push("resume".to_string());
        if codex.resume_last {
            args.push("--last".to_string());
        }
        if let Some(session) = &codex.resume_session {
            args.push(session.clone());
        }
    }

    // Extra flags
    args.extend_from_slice(&common.extra_flags);

    args.push("-".to_string());
    args
}

/// Codex command name (platform-specific).
pub fn codex_command() -> &'static str {
    if cfg!(target_os = "windows") {
        "codex.cmd"
    } else {
        "codex"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ralph_core::options::{AgentOptions, CodexOptions, CommonOptions};
    use ralph_core::types::SandboxMode;
    use std::path::PathBuf;

    #[test]
    fn builds_basic_args() {
        let options = AgentOptions {
            common: CommonOptions {
                sandbox_mode: Some(SandboxMode::WorkspaceWrite),
                ..Default::default()
            },
            ..Default::default()
        };
        let args = build_codex_args("fix the bug", "gpt-5.3", &options);
        assert!(args.contains(&"exec".to_string()));
        assert!(args.contains(&"--sandbox".to_string()));
        assert!(args.contains(&"workspace-write".to_string()));
        assert!(args.contains(&"--model".to_string()));
        assert!(args.contains(&"gpt-5.3".to_string()));
        assert!(args.contains(&"--json".to_string()));
        assert!(args.last() == Some(&"-".to_string()));
    }

    #[test]
    fn builds_resume_args() {
        let options = AgentOptions {
            codex: CodexOptions {
                resume_last: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let args = build_codex_args("continue", "gpt-5", &options);
        assert!(args.contains(&"resume".to_string()));
        assert!(args.contains(&"--last".to_string()));
    }

    #[test]
    fn builds_fork_args() {
        let options = AgentOptions {
            codex: CodexOptions {
                fork_session: Some("sess-123".to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        let args = build_codex_args("branch off", "gpt-5", &options);
        assert!(args.contains(&"fork".to_string()));
        assert!(args.contains(&"sess-123".to_string()));
        assert!(!args.contains(&"resume".to_string()));
    }

    #[test]
    fn builds_image_and_search_args() {
        let options = AgentOptions {
            codex: CodexOptions {
                images: vec![PathBuf::from("screenshot.png")],
                search: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let args = build_codex_args("analyze", "gpt-5", &options);
        assert!(args.contains(&"--image".to_string()));
        assert!(args.contains(&"screenshot.png".to_string()));
        assert!(args.contains(&"--search".to_string()));
    }
}
