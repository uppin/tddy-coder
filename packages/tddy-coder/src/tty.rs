//! TTY detection for TUI vs plain mode dispatch.

/// Returns true when both stdin and stderr are terminals (TUI mode).
pub fn should_run_tui(stdin_is_terminal: bool, stderr_is_terminal: bool) -> bool {
    stdin_is_terminal && stderr_is_terminal
}

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::rstest;

    #[rstest]
    #[case::both_non_tty(false, false, false)]
    #[case::only_stdin_tty(true, false, false)]
    #[case::only_stderr_tty(false, true, false)]
    #[case::both_tty(true, true, true)]
    fn runs_tui_only_when_both_stdin_and_stderr_are_terminals(
        #[case] stdin: bool,
        #[case] stderr: bool,
        #[case] expected: bool,
    ) {
        // When
        let result = should_run_tui(stdin, stderr);

        // Then
        assert_eq!(result, expected, "stdin={stdin} stderr={stderr}");
    }
}
