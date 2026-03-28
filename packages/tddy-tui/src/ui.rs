//! Rendering utilities: status bar formatting, elapsed time, workflow session segment for the
//! activity prefix (spinner + segment before `Goal:`), and shared Virtual TUI / local TUI layout.

use std::borrow::Cow;
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

/// Status bar text when there is no active goal/state row (idle / waiting): `Goal: —`, `State: —`, `Ready`, agent, model.
pub fn format_status_bar_idle(agent: &str, model: &str) -> String {
    format!(
        "Goal: — │ State: — │ Ready │ {} {} │ PgUp/PgDn scroll",
        agent, model
    )
}

/// Returns the first hyphen-separated segment of a workflow session id (for example the first 8
/// hex characters of a canonical UUID), or the documented placeholder ([`SESSION_SEGMENT_PLACEHOLDER`])
/// when the id is absent or cannot be parsed under the chosen rules.
pub const SESSION_SEGMENT_PLACEHOLDER: &str = "\u{2014}";

/// Extract the display segment between the spinner and `Goal:` from an optional workflow session id.
///
/// For a standard UUID string, this is the substring before the first hyphen, validated as an
/// 8-character lowercase hex field. Missing, empty, or malformed ids use
/// [`SESSION_SEGMENT_PLACEHOLDER`] consistently.
pub fn first_hyphen_segment_of_workflow_session_id(session_id: Option<&str>) -> Cow<'static, str> {
    let Some(raw) = session_id.map(str::trim).filter(|s| !s.is_empty()) else {
        log::trace!("workflow session segment: no session id → placeholder");
        return Cow::Borrowed(SESSION_SEGMENT_PLACEHOLDER);
    };

    let first_field = raw
        .split_once('-')
        .map(|(before, _)| before)
        .unwrap_or(raw)
        .trim();

    if first_field.len() == 8 && first_field.chars().all(|c| c.is_ascii_hexdigit()) {
        let normalized = first_field.to_ascii_lowercase();
        log::trace!(
            "workflow session segment: extracted UUID field {:?}",
            normalized
        );
        return Cow::Owned(normalized);
    }

    log::trace!(
        "workflow session segment: not a UUID first field (raw={raw:?}, field={first_field:?}) → placeholder"
    );
    Cow::Borrowed(SESSION_SEGMENT_PLACEHOLDER)
}

/// Prepends the spinner character and session segment to an already-formatted status tail (running
/// or idle layout).
pub(crate) fn prepend_activity_to_status_line(
    spinner_frame: char,
    session_segment: &str,
    tail: &str,
) -> String {
    log::trace!(
        "prepend_activity_to_status_line: spinner={:?} segment_len={}",
        spinner_frame,
        session_segment.chars().count()
    );
    format!("{} {} {}", spinner_frame, session_segment, tail)
}

/// Full status bar text with a leading spinner frame and session segment before the existing
/// `Goal:` … tail. Used by the primary TUI and Virtual TUI so remote clients match local layout.
pub fn format_status_bar_with_activity_prefix(
    spinner_frame: char,
    session_segment: &str,
    goal: &str,
    state: &str,
    elapsed: Duration,
    agent: &str,
    model: &str,
) -> String {
    let tail = format_status_bar(goal, state, elapsed, agent, model);
    prepend_activity_to_status_line(spinner_frame, session_segment, &tail)
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

    /// Acceptance (PRD): parsed segment equals the first 8 hex chars for a canonical UUID input.
    #[test]
    fn first_segment_matches_uuid_prefix_before_hyphen() {
        assert_eq!(
            first_hyphen_segment_of_workflow_session_id(Some(
                "550e8400-e29b-41d4-a716-446655440000"
            )),
            "550e8400"
        );
    }

    #[test]
    fn first_hyphen_segment_none_or_empty_returns_placeholder() {
        assert_eq!(
            first_hyphen_segment_of_workflow_session_id(None),
            SESSION_SEGMENT_PLACEHOLDER
        );
        assert_eq!(
            first_hyphen_segment_of_workflow_session_id(Some("")),
            SESSION_SEGMENT_PLACEHOLDER
        );
        assert_eq!(
            first_hyphen_segment_of_workflow_session_id(Some("   ")),
            SESSION_SEGMENT_PLACEHOLDER
        );
    }

    #[test]
    fn first_hyphen_segment_malformed_or_opaque_returns_placeholder() {
        assert_eq!(
            first_hyphen_segment_of_workflow_session_id(Some("not-a-uuid")),
            SESSION_SEGMENT_PLACEHOLDER
        );
        assert_eq!(
            first_hyphen_segment_of_workflow_session_id(Some("abcd-0000")),
            SESSION_SEGMENT_PLACEHOLDER
        );
    }

    #[test]
    fn first_hyphen_segment_uppercase_uuid_normalizes() {
        assert_eq!(
            first_hyphen_segment_of_workflow_session_id(Some(
                "550E8400-E29B-41D4-A716-446655440000"
            )),
            "550e8400"
        );
    }

    #[test]
    fn format_status_bar_idle_matches_running_tail_shape() {
        let idle = format_status_bar_idle("a", "m");
        assert!(idle.starts_with("Goal: — │ State: — │ Ready │ a m │"));
    }

    /// Granular Red: composed line must start with the spinner frame (stub omits it).
    #[test]
    fn format_status_bar_with_activity_prefix_leads_with_spinner_frame() {
        let line = format_status_bar_with_activity_prefix(
            '|',
            "\u{2014}",
            "g",
            "s",
            Duration::ZERO,
            "a",
            "m",
        );
        assert!(
            line.starts_with('|'),
            "expected line to start with spinner frame, got {line:?}"
        );
    }

    /// Acceptance (PRD): full status string contains substrings in order: spinner character,
    /// segment or placeholder, `Goal:`.
    #[test]
    fn status_bar_text_orders_spinner_segment_then_goal() {
        let line = format_status_bar_with_activity_prefix(
            '/',
            "550e8400",
            "plan",
            "Running",
            Duration::from_secs(3),
            "agent",
            "model",
        );
        let spin = line.find('/').expect("spinner frame");
        let seg = line.find("550e8400").expect("session segment");
        let goal = line.find("Goal:").expect("Goal:");
        assert!(
            spin < seg && seg < goal,
            "expected order / … 550e8400 … Goal:, got {line:?}"
        );
    }
}
