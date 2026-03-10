#[cfg(test)]
mod tests {
    use ralph_wiggum_rs::*;
    use std::path::PathBuf;

    #[test]
    fn test_completion_detection() {
        let output = "Some output\n<promise>DONE</promise>";
        assert!(check_terminal_promise(output, "DONE"));
    }

    #[test]
    fn test_tasks_completion() {
        let complete = "- [x] Task 1\n- [x] Task 2";
        assert!(tasks_markdown_all_complete(complete));

        let incomplete = "- [x] Task 1\n- [ ] Task 2";
        assert!(!tasks_markdown_all_complete(incomplete));
    }

    #[test]
    fn test_ansi_stripping() {
        let input = "\x1b[31mRed\x1b[0m text";
        assert_eq!(strip_ansi(input), "Red text");
    }

    #[test]
    fn test_codex_build_args_include_model_and_permissions() {
        let agent = agent::create_default_agent(AgentType::Codex);
        let options = AgentBuildArgsOptions {
            allow_all_permissions: false,
            codex_resume_last: false,
            codex_resume_session: None,
            extra_flags: Vec::new(),
            stream_output: true,
            sandbox_mode: Some(SandboxMode::WorkspaceWrite),
            approval_policy: Some(ApprovalPolicy::Never),
            extra_writable_dirs: vec![PathBuf::from("extra-dir")],
            output_last_message_path: Some(PathBuf::from("last-message.txt")),
        };

        let args = agent.build_args("probe", "claude-sonnet-4", &options);

        assert_eq!(
            args,
            vec![
                "exec",
                "--sandbox",
                "workspace-write",
                "--model",
                "claude-sonnet-4",
                "--add-dir",
                "extra-dir",
                "--output-last-message",
                "last-message.txt",
                "--json",
                "-",
            ]
        );
    }

    #[test]
    fn test_codex_build_args_omit_model_when_unset() {
        let agent = agent::create_default_agent(AgentType::Codex);
        let options = AgentBuildArgsOptions {
            allow_all_permissions: false,
            codex_resume_last: false,
            codex_resume_session: None,
            extra_flags: Vec::new(),
            stream_output: true,
            sandbox_mode: Some(SandboxMode::WorkspaceWrite),
            approval_policy: Some(ApprovalPolicy::Never),
            extra_writable_dirs: Vec::new(),
            output_last_message_path: None,
        };

        let args = agent.build_args("probe", "", &options);

        assert_eq!(
            args,
            vec!["exec", "--sandbox", "workspace-write", "--json", "-"]
        );
    }

    #[test]
    fn test_agent_specific_default_models() {
        assert_eq!(AgentType::Opencode.default_model(), Some("claude-sonnet-4"));
        assert_eq!(
            AgentType::ClaudeCode.default_model(),
            Some("claude-sonnet-4")
        );
        assert_eq!(AgentType::Codex.default_model(), None);
        assert_eq!(AgentType::Copilot.default_model(), None);
    }

    #[test]
    fn test_inject_prev_ai_context_adds_system_and_prev_ai_blocks() {
        let prompt = inject_prev_ai_context("继续开发", Some("上一轮已完成基础架构"));

        assert!(prompt.starts_with("<system>"));
        assert!(prompt.contains(PREV_AI_SYSTEM_PROMPT));
        assert!(prompt.contains("<prev-ai>\n上一轮已完成基础架构\n</prev-ai>"));
        assert!(prompt.ends_with("继续开发"));
    }
}
