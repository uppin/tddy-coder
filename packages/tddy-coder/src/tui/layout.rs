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

/// Compute the height (in lines) for the inbox display region.
///
/// Returns 0 when the inbox is empty or the TUI is not in Running mode.
/// Otherwise returns `min(item_count, 5)` to cap the visible area.
pub fn inbox_height(item_count: usize, is_running: bool) -> u16 {
    if item_count == 0 || !is_running {
        0
    } else {
        item_count.min(5) as u16
    }
}

/// Height for the debug log region. Returns 0 when no logs, else min(count, 5).
pub fn debug_log_height(log_count: usize) -> u16 {
    if log_count == 0 {
        0
    } else {
        log_count.min(5) as u16
    }
}

/// Split the terminal area into six regions, including inbox and optional debug log.
///
/// Returns `(activity_log, status_spacer, inbox_area, status_bar, debug_log, prompt_bar)`.
/// - debug_log: height 0 when no logs; shows buffered log lines when TUI mode + --debug
pub fn layout_chunks_with_inbox(
    area: Rect,
    inbox_h: u16,
    debug_log_h: u16,
) -> (Rect, Rect, Rect, Rect, Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(inbox_h),
            Constraint::Length(1),
            Constraint::Length(debug_log_h),
            Constraint::Length(1),
        ])
        .split(area);
    (
        chunks[0], chunks[1], chunks[2], chunks[3], chunks[4], chunks[5],
    )
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
        assert_eq!(
            spacer.y,
            activity.y + activity.height,
            "spacer must follow activity"
        );
        assert_eq!(
            status.y,
            spacer.y + spacer.height,
            "status_bar must follow spacer"
        );
        assert_eq!(
            prompt.y,
            status.y + status.height,
            "prompt_bar must follow status"
        );
        assert_eq!(
            activity.height + spacer.height + status.height + prompt.height,
            area.height,
            "regions must fill the area"
        );
    }

    /// AC2 + AC3: Inbox height is 0 when empty or not running, and equals
    /// min(item_count, 5) when running with items.
    #[test]
    fn test_inbox_not_rendered_when_empty() {
        // AC3: 0 height when empty
        assert_eq!(
            inbox_height(0, true),
            0,
            "inbox height must be 0 when item count is 0"
        );
        assert_eq!(
            inbox_height(0, false),
            0,
            "inbox height must be 0 when item count is 0 and not running"
        );
        // AC3: 0 height when not running, even with items
        assert_eq!(
            inbox_height(3, false),
            0,
            "inbox height must be 0 when not in Running mode"
        );
        // AC2: positive height when running with items
        assert_eq!(inbox_height(1, true), 1, "1 item must give height 1");
        assert_eq!(inbox_height(3, true), 3, "3 items must give height 3");
        assert_eq!(inbox_height(5, true), 5, "5 items must give height 5");
        assert_eq!(
            inbox_height(10, true),
            5,
            "10 items must be capped at height 5"
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
