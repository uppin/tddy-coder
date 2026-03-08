//! Layout computation for the TUI: three regions (activity log, status bar, prompt bar).
//!
//! AC1: activity log (scrollable, top), status bar (1 line, middle), prompt bar (fixed, bottom).

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Split the terminal area into four regions.
///
/// Returns `(activity_log, status_spacer, status_bar, prompt_bar)`.
/// - activity_log: scrollable, takes remaining space
/// - status_spacer: 1 blank line before status bar
/// - status_bar: exactly 1 line
/// - prompt_bar: at least 1 line for input
pub fn layout_chunks(area: Rect) -> (Rect, Rect, Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);
    (chunks[0], chunks[1], chunks[2], chunks[3])
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::prelude::Rect;

    /// AC1: Layout returns four non-overlapping regions.
    /// Activity log (top), spacer, status bar, prompt bar (bottom).
    #[test]
    fn test_layout_chunks_returns_four_regions() {
        let area = Rect::new(0, 0, 80, 24);
        let (activity, spacer, status, prompt) = layout_chunks(area);

        assert!(activity.width > 0, "activity_log must have width");
        assert!(activity.height > 0, "activity_log must have height");
        assert_eq!(spacer.height, 1, "status_spacer must be 1 line");
        assert_eq!(status.height, 1, "status_bar must be exactly 1 line");
        assert!(prompt.height >= 1, "prompt_bar must be at least 1 line");

        assert_eq!(activity.y, 0, "activity_log must start at top");
        assert_eq!(spacer.y, activity.y + activity.height, "spacer must follow activity");
        assert_eq!(status.y, spacer.y + spacer.height, "status_bar must follow spacer");
        assert_eq!(prompt.y, status.y + status.height, "prompt_bar must follow status");
        assert_eq!(
            activity.height + spacer.height + status.height + prompt.height,
            area.height,
            "regions must fill the area"
        );
    }

    /// AC1: Status bar displays between spacer and prompt.
    #[test]
    fn test_layout_status_bar_is_one_line() {
        let area = Rect::new(0, 0, 60, 20);
        let (_activity, _spacer, status, _prompt) = layout_chunks(area);

        assert_eq!(status.height, 1);
        assert_eq!(status.width, area.width);
    }
}
