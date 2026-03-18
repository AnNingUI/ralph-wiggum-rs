/// OpenCode-specific prompt enhancements.
///
/// Provides system prompts and prompt transformations for OpenCode agent.
pub const OPENCODE_SYSTEM_PROMPT: &str = "You are an AI coding assistant powered by OpenCode. \
     Use the available tools to read, write, and modify files. \
     When editing files, prefer using the edit tool over shell commands. \
     Always verify your changes and provide clear explanations.";

pub const OPENCODE_FILE_EDIT_PROMPT: &str = "For file operations, use the built-in file tools directly. \
     Do not use shell redirection or heredoc to write files. \
     Use the edit tool for precise modifications.";

/// Prepend OpenCode system prompt to user prompt.
pub fn prepend_opencode_system_prompt(prompt: &str) -> String {
    format!("<system>{}</system>\n\n{}", OPENCODE_SYSTEM_PROMPT, prompt)
}

/// Prepend file edit guidance to user prompt.
pub fn prepend_opencode_file_edit_prompt(prompt: &str) -> String {
    format!(
        "<system>{}</system>\n\n{}",
        OPENCODE_FILE_EDIT_PROMPT, prompt
    )
}

/// Prepend both system and file edit prompts.
pub fn prepend_opencode_full_prompt(prompt: &str) -> String {
    format!(
        "<system>{}\n\n{}</system>\n\n{}",
        OPENCODE_SYSTEM_PROMPT, OPENCODE_FILE_EDIT_PROMPT, prompt
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepend_system_adds_guidance() {
        let prompt = prepend_opencode_system_prompt("implement feature");
        assert!(prompt.contains(OPENCODE_SYSTEM_PROMPT));
        assert!(prompt.ends_with("implement feature"));
    }

    #[test]
    fn prepend_file_edit_adds_guidance() {
        let prompt = prepend_opencode_file_edit_prompt("edit main.rs");
        assert!(prompt.contains(OPENCODE_FILE_EDIT_PROMPT));
        assert!(prompt.ends_with("edit main.rs"));
    }

    #[test]
    fn prepend_full_adds_both() {
        let prompt = prepend_opencode_full_prompt("refactor code");
        assert!(prompt.contains(OPENCODE_SYSTEM_PROMPT));
        assert!(prompt.contains(OPENCODE_FILE_EDIT_PROMPT));
        assert!(prompt.ends_with("refactor code"));
    }
}
