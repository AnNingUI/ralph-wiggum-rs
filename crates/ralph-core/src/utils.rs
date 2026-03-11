use std::path::Path;
use std::process::{Command, Stdio};

pub const PREV_AI_SYSTEM_PROMPT: &str = "你现在再ralph模式下无线单用户提示词的循环迭代中，prev-ai块是上一次的结论，请继续完成用户发布的任务，并在完成了你的任务后提交本次任务的总结方便下一轮AI承接你的工作";

pub fn format_duration(ms: u64) -> String {
    let seconds = ms / 1000;
    let minutes = seconds / 60;
    let hours = minutes / 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes % 60, seconds % 60)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds % 60)
    } else {
        format!("{}s", seconds)
    }
}

pub async fn command_exists(cmd: &str) -> bool {
    #[cfg(target_os = "windows")]
    let which_cmd = "where";
    #[cfg(not(target_os = "windows"))]
    let which_cmd = "which";

    tokio::process::Command::new(which_cmd)
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn command_exists_blocking(cmd: &str) -> bool {
    #[cfg(target_os = "windows")]
    let which_cmd = "where";
    #[cfg(not(target_os = "windows"))]
    let which_cmd = "which";

    Command::new(which_cmd)
        .arg(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub async fn get_file_mtime(path: &Path) -> anyhow::Result<std::time::SystemTime> {
    let metadata = tokio::fs::metadata(path).await?;
    Ok(metadata.modified()?)
}

pub fn truncate_string(s: &str, max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }

    let char_count = s.chars().count();
    if char_count <= max_len {
        return s.to_string();
    }

    if max_len <= 3 {
        return s.chars().take(max_len).collect();
    }

    let take_len = max_len - 3;
    let mut truncated: String = s.chars().take(take_len).collect();
    truncated.push_str("...");
    truncated
}

fn sanitize_prev_ai_content(content: &str) -> String {
    content
        .replace("<system>", "<system-content>")
        .replace("</system>", "</system-content>")
        .replace("<prev-ai>", "<prev-ai-content>")
        .replace("</prev-ai>", "</prev-ai-content>")
}

pub fn inject_prev_ai_context(prompt: &str, prev_ai: Option<&str>) -> String {
    let Some(prev_ai) = prev_ai.map(str::trim).filter(|value| !value.is_empty()) else {
        return prompt.to_string();
    };

    format!(
        "<system>{}</system>\n<prev-ai>\n{}\n</prev-ai>\n\n{}",
        PREV_AI_SYSTEM_PROMPT,
        sanitize_prev_ai_content(prev_ai),
        prompt
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(500), "0s");
        assert_eq!(format_duration(5000), "5s");
        assert_eq!(format_duration(65000), "1m 5s");
        assert_eq!(format_duration(3665000), "1h 1m 5s");
    }

    #[test]
    fn test_truncate_string() {
        assert_eq!(truncate_string("short", 10), "short");
        assert_eq!(truncate_string("this is a long string", 10), "this is...");
    }

    #[test]
    fn test_inject_prev_ai_context_without_previous_response() {
        assert_eq!(inject_prev_ai_context("finish task", None), "finish task");
    }

    #[test]
    fn test_inject_prev_ai_context_with_previous_response() {
        let prompt = inject_prev_ai_context("finish task", Some("总结内容"));
        assert!(prompt.contains("<system>"));
        assert!(prompt.contains(PREV_AI_SYSTEM_PROMPT));
        assert!(prompt.contains("<prev-ai>\n总结内容\n</prev-ai>"));
        assert!(prompt.ends_with("finish task"));
    }

    #[test]
    fn test_inject_prev_ai_context_sanitizes_reserved_tags() {
        let prompt = inject_prev_ai_context("finish task", Some("<prev-ai>nested</prev-ai>"));
        assert!(prompt.contains("<prev-ai-content>nested</prev-ai-content>"));
    }
}
