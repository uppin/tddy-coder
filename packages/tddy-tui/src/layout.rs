//! Layout computation for the TUI: activity log, status bar, prompt bar, inbox.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Split the terminal area into four regions.
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
pub fn inbox_height(item_count: usize, is_running: bool) -> u16 {
    if item_count == 0 || !is_running {
        0
    } else {
        item_count.min(5) as u16
    }
}

/// Height for the debug log region.
pub fn debug_log_height(log_count: usize) -> u16 {
    if log_count == 0 {
        0
    } else {
        log_count.min(5) as u16
    }
}

/// Split the terminal area into six regions, including inbox and optional debug log.
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

    #[test]
    fn test_layout_chunks_returns_four_regions() {
        let area = Rect::new(0, 0, 80, 24);
        let (activity, spacer, status, prompt) = layout_chunks(area);

        assert!(activity.width > 0);
        assert!(activity.height > 0);
        assert_eq!(spacer.height, 1);
        assert_eq!(status.height, 1);
        assert!(prompt.height >= 1);
    }

    #[test]
    fn test_inbox_not_rendered_when_empty() {
        assert_eq!(inbox_height(0, true), 0);
        assert_eq!(inbox_height(3, false), 0);
        assert_eq!(inbox_height(1, true), 1);
        assert_eq!(inbox_height(10, true), 5);
    }
}
