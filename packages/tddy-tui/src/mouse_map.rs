//! Map mouse events to view-local actions and UserIntent.
//!
//! Hit-tests against layout areas to determine which UI element was clicked.
//! Scroll events adjust scroll offsets; clicks in dynamic_area map to option selection.

use crossterm::event::{MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use tddy_core::{AppMode, UserIntent};

use crate::view_state::ViewState;

fn rect_contains(r: &Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

/// Hit-testable layout areas from the last draw.
#[derive(Debug, Clone)]
pub struct LayoutAreas {
    pub activity_log: Rect,
    pub dynamic_area: Rect,
    pub status_bar: Rect,
    pub prompt_bar: Rect,
}

/// Normalize mouse coordinates for local terminal (crossterm).
/// Some terminals report row 1 less than actual; this corrects the off-by-one.
pub fn normalize_mouse_coords_for_local(event: MouseEvent) -> MouseEvent {
    MouseEvent {
        row: event.row.saturating_add(1),
        ..event
    }
}

/// Map a mouse event to a UserIntent or view-local action.
/// Updates view_state for selection changes; returns UserIntent when an action is triggered.
/// `inbox_len` is the number of inbox items (for Running mode scroll).
pub fn handle_mouse_event(
    event: MouseEvent,
    mode: &AppMode,
    view_state: &mut ViewState,
    areas: &LayoutAreas,
    inbox_len: usize,
) -> Option<UserIntent> {
    let col = event.column;
    let row = event.row;

    match event.kind {
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            let delta: i32 = match event.kind {
                MouseEventKind::ScrollUp => -1,
                MouseEventKind::ScrollDown => 1,
                _ => 0,
            };
            if rect_contains(&areas.activity_log, col, row) {
                view_state.scroll_offset =
                    view_state.scroll_offset.saturating_add_signed(delta as isize);
                return None;
            }
            if rect_contains(&areas.dynamic_area, col, row) {
                return scroll_dynamic_area(mode, view_state, areas, inbox_len, delta);
            }
        }
        MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
            if rect_contains(&areas.dynamic_area, col, row) {
                return click_dynamic_area(mode, view_state, areas, row);
            }
        }
        _ => {}
    }
    None
}

fn scroll_dynamic_area(
    mode: &AppMode,
    view_state: &mut ViewState,
    _areas: &LayoutAreas,
    inbox_len: usize,
    delta: i32,
) -> Option<UserIntent> {
    match mode {
        AppMode::MarkdownViewer { .. } => {
            view_state.markdown_scroll_offset = view_state
                .markdown_scroll_offset
                .saturating_add_signed(delta as isize);
            None
        }
        AppMode::Select { question, .. } => {
            let max = question.options.len() + if question.allow_other { 1 } else { 0 };
            let new_idx = (view_state.select_selected as i32 + delta)
                .clamp(0, max.saturating_sub(1) as i32) as usize;
            view_state.select_selected = new_idx;
            None
        }
        AppMode::MultiSelect { question, .. } => {
            let max = question.options.len() + if question.allow_other { 1 } else { 0 };
            let new_idx = (view_state.multiselect_cursor as i32 + delta)
                .clamp(0, max.saturating_sub(1) as i32) as usize;
            view_state.multiselect_cursor = new_idx;
            None
        }
        AppMode::PlanReview { .. } => {
            let new_idx =
                (view_state.plan_review_selected as i32 + delta).clamp(0, 2) as usize;
            view_state.plan_review_selected = new_idx;
            None
        }
        AppMode::ErrorRecovery { .. } => {
            let new_idx =
                (view_state.error_recovery_selected as i32 + delta).clamp(0, 2) as usize;
            view_state.error_recovery_selected = new_idx;
            None
        }
        AppMode::Running => {
            if view_state.inbox_focus == crate::view_state::InboxFocus::List {
                let max = inbox_len.saturating_sub(1).max(view_state.inbox_cursor);
                let new_idx =
                    (view_state.inbox_cursor as i32 + delta).clamp(0, max as i32) as usize;
                view_state.inbox_cursor = new_idx;
            }
            None
        }
        _ => None,
    }
}

fn click_dynamic_area(
    mode: &AppMode,
    view_state: &mut ViewState,
    areas: &LayoutAreas,
    row: u16,
) -> Option<UserIntent> {
    let row_offset = row.saturating_sub(areas.dynamic_area.y) as usize;

    match mode {
        AppMode::Select { question, .. } => {
            let header_lines = 2;
            if row_offset < header_lines {
                return None;
            }
            let option_idx = row_offset - header_lines;
            let max = question.options.len() + if question.allow_other { 1 } else { 0 };
            if option_idx >= max {
                return None;
            }
            view_state.select_selected = option_idx;
            None
        }
        AppMode::PlanReview { .. } => {
            let header_lines = 1;
            if row_offset < header_lines {
                return None;
            }
            let option_idx = row_offset - header_lines;
            if option_idx > 2 {
                return None;
            }
            view_state.plan_review_selected = option_idx;
            if option_idx == 0 {
                Some(UserIntent::ViewPlan)
            } else if option_idx == 1 {
                Some(UserIntent::ApprovePlan)
            } else {
                Some(UserIntent::RefinePlan)
            }
        }
        AppMode::ErrorRecovery { .. } => {
            let header_lines = 2;
            if row_offset < header_lines {
                return None;
            }
            let option_idx = row_offset - header_lines;
            if option_idx > 2 {
                return None;
            }
            view_state.error_recovery_selected = option_idx;
            if option_idx == 0 {
                Some(UserIntent::ResumeFromError)
            } else if option_idx == 1 {
                Some(UserIntent::ContinueWithAgent)
            } else {
                Some(UserIntent::Quit)
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::MouseButton;
    use crate::layout::{layout_chunks_with_inbox, question_height};
    use ratatui::layout::Rect;

    fn areas_80x24() -> LayoutAreas {
        let activity = Rect::new(0, 0, 80, 18);
        let dynamic = Rect::new(0, 18, 80, 4);
        let status = Rect::new(0, 22, 80, 1);
        let prompt = Rect::new(0, 23, 80, 1);
        LayoutAreas {
            activity_log: activity,
            dynamic_area: dynamic,
            status_bar: status,
            prompt_bar: prompt,
        }
    }

    /// Compute layout the same way draw() does for PlanReview on 80x24.
    /// Ensures tests use the real layout, not a stale fixture.
    fn areas_from_real_layout_80x24_plan_review() -> LayoutAreas {
        let area = Rect::new(0, 0, 80, 24);
        let mode = AppMode::PlanReview {
            prd_content: String::new(),
        };
        let dynamic_h = question_height(&mode);
        let debug_h = 0u16;
        let prompt_h = 1u16;
        let (activity_log, _spacer, dynamic_area, status_bar, _debug, prompt_bar) =
            layout_chunks_with_inbox(area, dynamic_h, debug_h, prompt_h);
        LayoutAreas {
            activity_log,
            dynamic_area,
            status_bar,
            prompt_bar,
        }
    }

    /// Fixture must match real layout for PlanReview 80x24. Fails if layout drifts.
    #[test]
    fn fixture_matches_real_layout_plan_review_80x24() {
        let fixture = areas_80x24();
        let real = areas_from_real_layout_80x24_plan_review();
        assert_eq!(
            fixture.dynamic_area.y, real.dynamic_area.y,
            "dynamic_area.y must match: fixture={} real={}. Layout drift causes off-by-one.",
            fixture.dynamic_area.y, real.dynamic_area.y
        );
        assert_eq!(fixture.dynamic_area.height, real.dynamic_area.height);
    }

    /// When terminal sends row 19 for Approve click, caller must normalize before handle_mouse_event.
    #[test]
    fn click_row_19_when_terminal_sends_1_less_must_select_approve() {
        let mut vs = ViewState::new();
        let areas = areas_80x24();
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 19,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let ev_normalized = normalize_mouse_coords_for_local(ev);
        let mode = AppMode::PlanReview {
            prd_content: "plan".to_string(),
        };
        let intent = handle_mouse_event(ev_normalized, &mode, &mut vs, &areas, 0);
        assert_eq!(
            vs.plan_review_selected, 1,
            "When terminal sends row 19 for Approve click, must select Approve (off-by-one bug)"
        );
        assert!(matches!(intent, Some(UserIntent::ApprovePlan)));
    }

    #[test]
    fn scroll_in_activity_log_adjusts_scroll_offset() {
        let mut vs = ViewState::new();
        let areas = areas_80x24();
        let ev = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 10,
            row: 5,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let intent = handle_mouse_event(ev, &AppMode::Running, &mut vs, &areas, 0);
        assert!(intent.is_none());
        assert_eq!(vs.scroll_offset, 1);
    }

    #[test]
    fn click_plan_review_approve_produces_intent() {
        let mut vs = ViewState::new();
        let areas = areas_80x24();
        // dynamic_area.y=18: row 18=header, 19=View, 20=Approve, 21=Refine
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 20,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let mode = AppMode::PlanReview {
            prd_content: "plan".to_string(),
        };
        let intent = handle_mouse_event(ev, &mode, &mut vs, &areas, 0);
        assert_eq!(vs.plan_review_selected, 1);
        assert!(matches!(intent, Some(UserIntent::ApprovePlan)));
    }

    #[test]
    fn scroll_in_dynamic_area_navigates_plan_review() {
        let mut vs = ViewState::new();
        vs.plan_review_selected = 1;
        let areas = areas_80x24();
        let ev = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 5,
            row: 20,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let mode = AppMode::PlanReview {
            prd_content: "plan".to_string(),
        };
        let intent = handle_mouse_event(ev, &mode, &mut vs, &areas, 0);
        assert!(intent.is_none());
        assert_eq!(vs.plan_review_selected, 0);
    }

    /// Regression: mouse click must select the option at the clicked row, not 1 line above.
    /// dynamic_area.y=18: row 18=header, 19=View, 20=Approve, 21=Refine.
    #[test]
    fn click_plan_review_refine_selects_refine_not_approve() {
        let mut vs = ViewState::new();
        let areas = areas_80x24();
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 21,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let mode = AppMode::PlanReview {
            prd_content: "plan".to_string(),
        };
        let intent = handle_mouse_event(ev, &mode, &mut vs, &areas, 0);
        assert_eq!(
            vs.plan_review_selected, 2,
            "clicking Refine row (21) must select option 2, not option 1 (off-by-one bug)"
        );
        assert!(
            matches!(intent, Some(UserIntent::RefinePlan)),
            "clicking Refine row must produce RefinePlan intent, not ApprovePlan"
        );
    }

    /// Regression: click on View (first option) must select View.
    #[test]
    fn click_plan_review_view_selects_view() {
        let mut vs = ViewState::new();
        let areas = areas_80x24();
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 19,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let mode = AppMode::PlanReview {
            prd_content: "plan".to_string(),
        };
        let intent = handle_mouse_event(ev, &mode, &mut vs, &areas, 0);
        assert_eq!(vs.plan_review_selected, 0);
        assert!(matches!(intent, Some(UserIntent::ViewPlan)));
    }
}
