//! Rendering utilities for the TUI: status bar formatting, elapsed time, layout.

use std::time::Duration;

use ratatui::style::{Color, Modifier, Style};

/// Format an elapsed duration as a compact human-readable string.
///
/// Rules:
/// - < 60 s → "{s}s"           e.g. "0s", "45s", "59s"
/// - < 1 h  → "{m}m {s}s"     e.g. "1m 0s", "2m 34s"
/// - ≥ 1 h  → "{h}h {m}m"     e.g. "1h 0m", "1h 5m"
pub fn format_elapsed(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        let m = secs / 60;
        let s = secs % 60;
        format!("{}m {}s", m, s)
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        format!("{}h {}m", h, m)
    }
}

/// Format the status bar line shown between the activity log and the prompt bar.
///
/// Expected format: `"Goal: {goal} │ State: {state} │ {elapsed} │ {agent} {model} │ PgUp/PgDn scroll"`
pub fn format_status_bar(
    goal: &str,
    state: &str,
    elapsed: Duration,
    agent: &str,
    model: &str,
) -> String {
    let elapsed_str = format_elapsed(elapsed);
    format!(
        "Goal: {} │ State: {} │ {} │ {} {} │ PgUp/PgDn scroll",
        goal, state, elapsed_str, agent, model
    )
}

/// Goal-specific background color for the status bar.
/// plan: yellow, acceptance-tests: orange, red: red, green: green, evaluate/validate: blue.
/// Text is bold white on colored background.
pub fn status_bar_style_for_goal(goal: Option<&str>) -> Style {
    let bg = match goal {
        Some("plan") => Color::Yellow,
        Some("acceptance-tests") => Color::Rgb(255, 165, 0),
        Some("red") => Color::Red,
        Some("green") => Color::Green,
        Some("evaluate")
        | Some("validate")
        | Some("validate-changes")
        | Some("validate-refactor") => Color::Blue,
        _ => Color::DarkGray,
    };
    Style::default()
        .fg(Color::White)
        .bg(bg)
        .add_modifier(Modifier::BOLD)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AC2: Elapsed time formats correctly at boundaries: 0s, <60s, <1h, ≥1h.
    #[test]
    fn test_elapsed_time_format() {
        assert_eq!(format_elapsed(Duration::ZERO), "0s");
        assert_eq!(format_elapsed(Duration::from_secs(1)), "1s");
        assert_eq!(format_elapsed(Duration::from_secs(45)), "45s");
        assert_eq!(format_elapsed(Duration::from_secs(59)), "59s");
        assert_eq!(format_elapsed(Duration::from_secs(60)), "1m 0s");
        assert_eq!(format_elapsed(Duration::from_secs(154)), "2m 34s");
        assert_eq!(format_elapsed(Duration::from_secs(3600)), "1h 0m");
        assert_eq!(format_elapsed(Duration::from_secs(3900)), "1h 5m");
        assert_eq!(format_elapsed(Duration::from_secs(7384)), "2h 3m");
    }

    /// AC2: Status bar contains goal, state, elapsed time, agent, model, and │ separators.
    #[test]
    fn test_status_bar_format() {
        let elapsed = Duration::from_secs(154); // "2m 34s"
        let result = format_status_bar("plan", "Planning", elapsed, "claude", "opus");

        assert!(
            result.contains("Goal: plan"),
            "status bar must contain 'Goal: plan': {}",
            result
        );
        assert!(
            result.contains("State: Planning"),
            "status bar must contain 'State: Planning': {}",
            result
        );
        assert!(
            result.contains("2m 34s"),
            "status bar must contain elapsed '2m 34s': {}",
            result
        );
        assert!(
            result.contains('│'),
            "status bar must use │ as separator: {}",
            result
        );

        // Verify agent and model appear
        assert!(result.contains("claude"), "status bar must contain agent");
        assert!(result.contains("opus"), "status bar must contain model");

        // Verify ordering: Goal appears before State, State before elapsed
        let goal_pos = result.find("Goal:").unwrap();
        let state_pos = result.find("State:").unwrap();
        let elapsed_pos = result.find("2m 34s").unwrap();
        assert!(goal_pos < state_pos, "Goal must appear before State");
        assert!(
            state_pos < elapsed_pos,
            "State must appear before elapsed time"
        );
    }
}
