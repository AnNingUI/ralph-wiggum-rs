pub const CODEX_FILE_EDIT_SYSTEM_PROMPT: &str = "For file edits, use the built-in apply_patch tool directly. Do not invoke apply_patch through shell/exec_command. Do not rely on shell multiline redirection, heredoc, or herestring to write files.";

pub fn prepend_codex_file_edit_prompt(prompt: &str) -> String {
    format!(
        "<system>{}</system>\n\n{}",
        CODEX_FILE_EDIT_SYSTEM_PROMPT, prompt
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepend_adds_guidance() {
        let prompt = prepend_codex_file_edit_prompt("finish the feature");
        assert!(prompt.contains(CODEX_FILE_EDIT_SYSTEM_PROMPT));
        assert!(prompt.ends_with("finish the feature"));
    }
}
