//! Rendering utilities: status bar formatting, elapsed time.

use std::time::Duration;

use ratatui::style::{Color, Modifier, Style};

/// Format an elapsed duration as a compact human-readable string.
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

/// Format the status bar line.
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
pub fn status_bar_style_for_goal(goal: Option<&str>) -> Style {
    let bg = match goal {
        Some("plan") => Color::Yellow,
        Some("acceptance-tests") => Color::Rgb(255, 165, 0),
        Some("red") => Color::Red,
        Some("green") => Color::Green,
        Some("evaluate") | Some("validate") => Color::Blue,
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

    #[test]
    fn test_elapsed_time_format() {
        assert_eq!(format_elapsed(Duration::ZERO), "0s");
        assert_eq!(format_elapsed(Duration::from_secs(60)), "1m 0s");
        assert_eq!(format_elapsed(Duration::from_secs(3600)), "1h 0m");
    }
}
