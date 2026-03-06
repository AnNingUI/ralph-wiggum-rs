#[cfg(test)]
mod tests {
    use ralph_wiggum_rs::*;

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
}
