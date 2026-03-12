use ralph_core::options::AgentOptions;

pub fn opencode_command() -> &'static str {
    if cfg!(target_os = "windows") {
        "opencode.cmd"
    } else {
        "opencode"
    }
}

pub fn build_opencode_args(
    prompt: &str,
    model: &str,
    options: &AgentOptions,
) -> Vec<String> {
    let mut args = vec!["run".to_string()];

    // Session management
    if options.opencode.continue_session {
        args.push("--continue".to_string());
    }

    if let Some(session) = &options.opencode.session_id {
        args.push("--session".to_string());
        args.push(session.clone());
    }

    if options.opencode.fork_session {
        args.push("--fork".to_string());
    }

    // Model configuration
    if !model.is_empty() {
        args.push("--model".to_string());
        args.push(model.to_string());
    }

    if let Some(variant) = &options.opencode.variant {
        args.push("--variant".to_string());
        args.push(variant.clone());
    }

    // Agent configuration
    if let Some(agent) = &options.opencode.agent {
        args.push("--agent".to_string());
        args.push(agent.clone());
    }

    // Output configuration
    if let Some(format) = &options.opencode.format {
        args.push("--format".to_string());
        args.push(format.clone());
    }

    if options.opencode.thinking {
        args.push("--thinking".to_string());
    }

    // Session metadata
    if let Some(title) = &options.opencode.title {
        args.push("--title".to_string());
        args.push(title.clone());
    }

    // Server configuration
    if let Some(attach) = &options.opencode.attach {
        args.push("--attach".to_string());
        args.push(attach.clone());
    }

    if let Some(dir) = &options.opencode.dir {
        args.push("--dir".to_string());
        args.push(dir.display().to_string());
    }

    if let Some(port) = options.opencode.port {
        args.push("--port".to_string());
        args.push(port.to_string());
    }

    // File attachments
    for file in &options.opencode.files {
        args.push("--file".to_string());
        args.push(file.display().to_string());
    }

    // Add the prompt
    args.push(prompt.to_string());

    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use ralph_core::options::OpencodeOptions;

    #[test]
    fn test_basic_args() {
        let mut options = AgentOptions::default();
        options.opencode = OpencodeOptions::default();

        let args = build_opencode_args("test prompt", "", &options);
        eprintln!("Generated args: {:?}", args);

        assert_eq!(args[0], "run");
        assert_eq!(args[args.len() - 1], "test prompt");
    }
}
