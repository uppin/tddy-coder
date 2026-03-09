//! TTY detection for TUI vs plain mode dispatch.

/// Returns true when both stdin and stderr are terminals (TUI mode).
pub fn should_run_tui(stdin_is_terminal: bool, stderr_is_terminal: bool) -> bool {
    stdin_is_terminal && stderr_is_terminal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tty_detection_dispatch() {
        assert!(!should_run_tui(false, false));
        assert!(!should_run_tui(true, false));
        assert!(!should_run_tui(false, true));
        assert!(should_run_tui(true, true));
    }
}
