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

    #[test]
    fn format_elapsed_compact_matches_tui_cases() {
        assert_eq!(format_elapsed_compact(Duration::ZERO), "0s");
        assert_eq!(format_elapsed_compact(Duration::from_secs(60)), "1m 0s");
        assert_eq!(format_elapsed_compact(Duration::from_secs(3600)), "1h 0m");
    }
}
