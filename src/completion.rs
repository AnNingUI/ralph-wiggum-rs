//! Completion detection helpers used by the Ralph loop.

use regex::Regex;
use std::sync::OnceLock;

static ANSI_PATTERN: OnceLock<Regex> = OnceLock::new();
static CHECKBOX_PATTERN: OnceLock<Regex> = OnceLock::new();

fn get_ansi_pattern() -> &'static Regex {
    ANSI_PATTERN.get_or_init(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap())
}

fn get_checkbox_pattern() -> &'static Regex {
    CHECKBOX_PATTERN.get_or_init(|| Regex::new(r"^\s*-\s+\[([ xX/])\]\s+").unwrap())
}

/// Strips ANSI escape codes from input string
pub fn strip_ansi(input: &str) -> String {
    get_ansi_pattern().replace_all(input, "").into_owned()
}

/// Escapes special regex characters
pub fn escape_regex(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        match c {
            '.' | '*' | '+' | '?' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\' => {
                result.push('\\');
                result.push(c);
            }
            _ => result.push(c),
        }
    }
    result
}

/// Returns the last non-empty line of output, after ANSI stripping
pub fn get_last_non_empty_line(output: &str) -> Option<String> {
    strip_ansi(output)
        .replace("\r\n", "\n")
        .split('\n')
        .rev()
        .map(|line| line.trim())
        .find(|line| !line.is_empty())
        .map(|s| s.to_string())
}

/// Checks whether the exact promise tag appears as the final non-empty line
pub fn check_terminal_promise(output: &str, promise: &str) -> bool {
    let last_line = match get_last_non_empty_line(output) {
        Some(line) => line,
        None => return false,
    };

    let escaped_promise = escape_regex(promise);
    let pattern = format!(r"(?i)^<promise>\s*{}\s*</promise>$", escaped_promise);

    match Regex::new(&pattern) {
        Ok(re) => re.is_match(&last_line),
        Err(_) => false,
    }
}

/// Returns true only when there is at least one task checkbox and all checkboxes are complete
pub fn tasks_markdown_all_complete(tasks_markdown: &str) -> bool {
    let pattern = get_checkbox_pattern();
    let mut saw_task = false;

    for line in tasks_markdown.lines() {
        if let Some(captures) = pattern.captures(line) {
            saw_task = true;
            if let Some(checkbox) = captures.get(1) {
                let checkbox_char = checkbox.as_str();
                if !checkbox_char.eq_ignore_ascii_case("x") {
                    return false;
                }
            }
        }
    }

    saw_task
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi() {
        let input = "\x1b[31mRed text\x1b[0m";
        assert_eq!(strip_ansi(input), "Red text");
    }

    #[test]
    fn test_escape_regex() {
        assert_eq!(escape_regex("test.txt"), "test\\.txt");
        assert_eq!(escape_regex("a*b+c?"), "a\\*b\\+c\\?");
    }

    #[test]
    fn test_get_last_non_empty_line() {
        let output = "line1\nline2\n\n";
        assert_eq!(get_last_non_empty_line(output), Some("line2".to_string()));
    }

    #[test]
    fn test_check_terminal_promise() {
        let output = "Some output\n<promise>DONE</promise>";
        assert!(check_terminal_promise(output, "DONE"));
        
        let output2 = "Some output\nNot done";
        assert!(!check_terminal_promise(output2, "DONE"));
    }

    #[test]
    fn test_tasks_markdown_all_complete() {
        let complete = "- [x] Task 1\n- [x] Task 2";
        assert!(tasks_markdown_all_complete(complete));
        
        let incomplete = "- [x] Task 1\n- [ ] Task 2";
        assert!(!tasks_markdown_all_complete(incomplete));
        
        let no_tasks = "No tasks here";
        assert!(!tasks_markdown_all_complete(no_tasks));
    }
}
