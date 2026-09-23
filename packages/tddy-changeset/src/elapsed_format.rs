//! Compact human-readable duration strings (TUI status bar and web session list).
//!
//! Matches `packages/tddy-tui/src/ui.rs::format_elapsed` semantics.

use std::time::Duration;

/// Format a non-negative duration as a compact human-readable string (`Ns`, `Nm Ns`, `Nh Nm`).
pub fn format_elapsed_compact(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        let m = secs / 60;
        let s = secs % 60;
        format!("{m}m {s}s")
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        format!("{h}h {m}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::zero_seconds(Duration::ZERO, "0s")]
    #[case::one_minute(Duration::from_secs(60), "1m 0s")]
    #[case::one_hour(Duration::from_secs(3600), "1h 0m")]
    fn formats_duration_compactly(#[case] input: Duration, #[case] expected: &str) {
        // When
        let result = format_elapsed_compact(input);

        // Then
        assert_eq!(result, expected);
    }
}
