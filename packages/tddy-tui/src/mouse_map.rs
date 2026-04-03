//! Map mouse events to view-local actions and UserIntent.
//!
//! Hit-tests against layout areas to determine which UI element was clicked.
//! Scroll events adjust scroll offsets; clicks in dynamic_area map to option selection.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind};
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
    /// Single-row footer directly below the prompt text block (PRD: +1 bottom chrome row).
    pub footer_bar: Rect,
    /// Dedicated terminal region for the pointer Enter affordance (must not overlap [`prompt_bar`]).
    pub enter_pane: Rect,
}

/// Width in terminal cells of the pointer Enter affordance (see `enter_button_rect`). Height is
/// computed from layout (see `enter_button_rect`): rows from below the status bar through prompt
/// text and footer (excludes the prompt rule row when present).
pub const ENTER_BUTTON_COLS: u16 = 3;
/// Empty columns between the prompt text block and the Enter strip (to the right of prompt text).
pub const ENTER_STRIP_MARGIN_COLS: u16 = 1;
/// Legacy export: pre-footer overlay used two content rows for the ASCII frame. Actual hit target
/// height is [`enter_button_rect`].`height` (prompt text + footer; status bar is excluded).
pub const ENTER_BUTTON_ROWS: u16 = 2;

/// **3 columns** wide: starts at the first row **below the status bar** (the separator band: empty
/// row plus any debug rows above [`LayoutAreas::prompt_bar`]), then spans **prompt text** + **footer**.
/// Placed to the right of the prompt text block with [`ENTER_STRIP_MARGIN_COLS`] gap. Does not cover
/// the status bar itself. When the prompt chunk has more than one row, the bottom row is the
/// horizontal rule (`U+2500`) and is **not** included in height. Matches
/// [`crate::render::paint_enter_affordance`].
pub fn enter_button_rect(areas: &LayoutAreas) -> Rect {
    crate::red_phase::tddy_marker("M002", "mouse_map::enter_button_rect");
    let sb = areas.status_bar;
    let pb = areas.prompt_bar;
    let fb = areas.footer_bar;
    if pb.width < ENTER_BUTTON_COLS {
        log::debug!("enter_button_rect: degenerate layout pb={pb:?} fb={fb:?} -> empty");
        return Rect::new(0, 0, 0, 0);
    }
    let prompt_rows = if pb.height > 1 {
        pb.height.saturating_sub(1)
    } else {
        pb.height
    };
    if prompt_rows == 0 && fb.height == 0 {
        log::debug!("enter_button_rect: zero-height prompt+footer pb={pb:?} fb={fb:?} -> empty");
        return Rect::new(0, 0, 0, 0);
    }
    let x = pb.x + pb.width + ENTER_STRIP_MARGIN_COLS;
    let (y, h) = if sb.height > 0 {
        let strip_top = sb.y.saturating_add(sb.height);
        let above_prompt = pb.y.saturating_sub(strip_top);
        let h = above_prompt
            .saturating_add(prompt_rows)
            .saturating_add(fb.height);
        (strip_top, h)
    } else {
        let h = prompt_rows.saturating_add(fb.height);
        (pb.y, h)
    };
    if h == 0 {
        return Rect::new(0, 0, 0, 0);
    }
    log::trace!(
        "enter_button_rect: x={x} y={y} w={} h={h} (from below status through prompt text+footer, excluding prompt rule row)",
        ENTER_BUTTON_COLS
    );
    Rect::new(x, y, ENTER_BUTTON_COLS, h)
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
                if matches!(mode, AppMode::MarkdownViewer { .. }) {
                    let h = areas.activity_log.height;
                    let footer_h = if h >= 2 { 2 } else { 1 }.min(h);
                    let footer_top = areas.activity_log.y + h - footer_h;
                    if h > 0 && row >= footer_top && row < areas.activity_log.y + h {
                        log::trace!("mouse: scroll on plan footer ignored");
                        return None;
                    }
                    view_state.markdown_scroll_offset = view_state
                        .markdown_scroll_offset
                        .saturating_add_signed(delta as isize);
                    log::trace!(
                        "mouse: markdown scroll offset={}",
                        view_state.markdown_scroll_offset
                    );
                    return None;
                }
                view_state.scroll_offset = view_state
                    .scroll_offset
                    .saturating_add_signed(delta as isize);
                return None;
            }
            if rect_contains(&areas.dynamic_area, col, row) {
                return scroll_dynamic_area(mode, view_state, areas, inbox_len, delta);
            }
        }
        MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
            if matches!(mode, AppMode::MarkdownViewer { .. })
                && rect_contains(&areas.activity_log, col, row)
            {
                let h = areas.activity_log.height;
                let footer_h = if h >= 2 { 2 } else { 1 }.min(h);
                let footer_top = areas.activity_log.y + h - footer_h;
                if h > 0 && row >= footer_top && row < areas.activity_log.y + h {
                    return plan_approval_activity_footer_click(view_state, areas, col, row);
                }
                log::trace!("mouse: click in markdown body (no intent)");
                return None;
            }
            if rect_contains(&areas.dynamic_area, col, row) {
                return click_dynamic_area(mode, view_state, areas, row);
            }
            if rect_contains(&enter_button_rect(areas), col, row) {
                return click_enter_affordance(mode, view_state);
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
        AppMode::DocumentReview { .. } => {
            let new_idx = (view_state.document_review_selected as i32 + delta).clamp(0, 2) as usize;
            view_state.document_review_selected = new_idx;
            None
        }
        AppMode::ErrorRecovery { .. } => {
            let new_idx = (view_state.error_recovery_selected as i32 + delta).clamp(0, 2) as usize;
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
            let is_double_click = view_state.last_select_click_option == Some(option_idx);
            view_state.select_selected = option_idx;
            view_state.last_select_click_option = Some(option_idx);
            if is_double_click && option_idx < question.options.len() {
                Some(UserIntent::AnswerSelect(option_idx))
            } else {
                None
            }
        }
        AppMode::DocumentReview { .. } => {
            let header_lines = 1;
            if row_offset < header_lines {
                return None;
            }
            let option_idx = row_offset - header_lines;
            if option_idx > 2 {
                return None;
            }
            view_state.document_review_selected = option_idx;
            if option_idx == 0 {
                Some(UserIntent::ViewSessionDocument)
            } else if option_idx == 1 {
                Some(UserIntent::ApproveSessionDocument)
            } else {
                Some(UserIntent::RefineSessionDocument)
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

fn click_enter_affordance(mode: &AppMode, view_state: &ViewState) -> Option<UserIntent> {
    let enter = KeyEvent::new_with_kind(KeyCode::Enter, KeyModifiers::empty(), KeyEventKind::Press);
    crate::key_map::key_event_to_intent(enter, mode, view_state, false)
}

/// Plan approval footer: two stacked lines (Approve then Reject) like Select options, or one row
/// split left/right when the activity pane is only one line tall.
fn plan_approval_activity_footer_click(
    view_state: &mut ViewState,
    areas: &LayoutAreas,
    col: u16,
    row: u16,
) -> Option<UserIntent> {
    let h = areas.activity_log.height;
    if h == 0 {
        return None;
    }
    let footer_h = if h >= 2 { 2 } else { 1 }.min(h);
    let footer_top = areas.activity_log.y + h - footer_h;
    log::debug!(
        "plan_approval_activity_footer_click: col={} row={} footer_top={} footer_h={} activity={:?}",
        col,
        row,
        footer_top,
        footer_h,
        areas.activity_log
    );
    if footer_h >= 2 {
        let rel = row.saturating_sub(footer_top);
        if rel == 0 {
            view_state.markdown_end_button_selected = 0;
            Some(UserIntent::ApproveSessionDocument)
        } else {
            view_state.markdown_end_button_selected = 1;
            Some(UserIntent::RefineSessionDocument)
        }
    } else {
        let mid = areas.activity_log.x + areas.activity_log.width / 2;
        if col < mid {
            view_state.markdown_end_button_selected = 0;
            Some(UserIntent::ApproveSessionDocument)
        } else {
            view_state.markdown_end_button_selected = 1;
            Some(UserIntent::RefineSessionDocument)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{layout_chunks_with_inbox, prompt_height, question_height};
    use crossterm::event::MouseButton;
    use ratatui::layout::Rect;

    /// Fixed rects for the **legacy** four-line PlanReview menu strip (pre–activity-pane redesign).
    /// Kept to unit-test [`click_dynamic_area`] / scroll behavior for [`AppMode::PlanReview`];
    /// real [`draw`] uses [`crate::layout::question_height`] (0 strip) — see
    /// [`areas_from_real_layout_80x24_plan_review`].
    fn legacy_plan_review_menu_strip_fixture_80x24() -> LayoutAreas {
        let activity = Rect::new(0, 0, 80, 18);
        let dynamic = Rect::new(0, 18, 80, 4);
        let status = Rect::new(0, 22, 80, 1);
        let prompt = Rect::new(0, 23, 80, 1);
        LayoutAreas {
            activity_log: activity,
            dynamic_area: dynamic,
            status_bar: status,
            prompt_bar: prompt,
            footer_bar: Rect::new(0, 24, 80, 0),
            enter_pane: Rect::default(),
        }
    }

    /// Compute layout the same way draw() does for DocumentReview on 80x24.
    /// Ensures tests use the real layout, not a stale fixture.
    fn areas_from_real_layout_80x24_document_review() -> LayoutAreas {
        let area = Rect::new(0, 0, 80, 24);
        let mode = AppMode::DocumentReview {
            content: String::new(),
        };
        let dynamic_h = question_height(&mode);
        let debug_h = 0u16;
        let prompt_h = 1u16;
        let (activity_log, _spacer, dynamic_area, status_bar, _gap, _debug, prompt_bar, footer_bar) =
            layout_chunks_with_inbox(area, dynamic_h, debug_h, prompt_h);
        LayoutAreas {
            activity_log,
            dynamic_area,
            status_bar,
            prompt_bar,
            footer_bar,
            enter_pane: Rect::default(),
        }
    }

    /// Same layout as `draw()` for ErrorRecovery on 80x24 (prompt height matches `render::draw`).
    fn areas_from_real_layout_80x24_error_recovery() -> LayoutAreas {
        let area = Rect::new(0, 0, 80, 24);
        let mode = AppMode::ErrorRecovery {
            error_message: "e".to_string(),
        };
        let is_running = false;
        let inbox_h = crate::layout::inbox_height(0, is_running);
        let question_h = question_height(&mode);
        let dynamic_h = question_h.max(inbox_h);
        let debug_h = 0u16;
        let prompt_text = "Up/Down navigate  Enter select";
        let text_len = prompt_text.chars().count().min(u16::MAX as usize) as u16;
        let area_width = area.width;
        let max_height = (area.height / 3).max(1);
        let prompt_h = prompt_height(text_len, area_width, max_height);
        let (activity_log, _spacer, dynamic_area, status_bar, _gap, _debug, prompt_bar, footer_bar) =
            layout_chunks_with_inbox(area, dynamic_h, debug_h, prompt_h);
        LayoutAreas {
            activity_log,
            dynamic_area,
            status_bar,
            prompt_bar,
            footer_bar,
            enter_pane: Rect::default(),
        }
    }

    /// Plan approval uses the activity region; the old four-line dynamic menu strip is not allocated.
    #[test]
    fn plan_review_real_layout_has_no_dynamic_menu_strip_80x24() {
        let real = areas_from_real_layout_80x24_document_review();
        assert_eq!(
            real.dynamic_area.height, 0,
            "DocumentReview must not reserve a separate dynamic strip for View/Approve/Refine"
        );
    }

    /// When terminal sends row 19 for Approve click, caller must normalize before handle_mouse_event.
    #[test]
    fn click_row_19_when_terminal_sends_1_less_must_select_approve() {
        let mut vs = ViewState::new();
        let areas = legacy_plan_review_menu_strip_fixture_80x24();
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 19,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let ev_normalized = normalize_mouse_coords_for_local(ev);
        let mode = AppMode::DocumentReview {
            content: "plan".to_string(),
        };
        let intent = handle_mouse_event(ev_normalized, &mode, &mut vs, &areas, 0);
        assert_eq!(
            vs.document_review_selected, 1,
            "When terminal sends row 19 for Approve click, must select Approve (off-by-one bug)"
        );
        assert!(matches!(intent, Some(UserIntent::ApproveSessionDocument)));
    }

    #[test]
    fn scroll_in_activity_log_adjusts_scroll_offset() {
        let mut vs = ViewState::new();
        let areas = legacy_plan_review_menu_strip_fixture_80x24();
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
    fn click_document_review_approve_produces_intent() {
        let mut vs = ViewState::new();
        let areas = legacy_plan_review_menu_strip_fixture_80x24();
        // dynamic_area.y=18: row 18=header, 19=View, 20=Approve, 21=Refine
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 20,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let mode = AppMode::DocumentReview {
            content: "plan".to_string(),
        };
        let intent = handle_mouse_event(ev, &mode, &mut vs, &areas, 0);
        assert_eq!(vs.document_review_selected, 1);
        assert!(matches!(intent, Some(UserIntent::ApproveSessionDocument)));
    }

    #[test]
    fn scroll_in_dynamic_area_navigates_document_review() {
        let mut vs = ViewState::new();
        vs.document_review_selected = 1;
        let areas = legacy_plan_review_menu_strip_fixture_80x24();
        let ev = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 5,
            row: 20,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let mode = AppMode::DocumentReview {
            content: "plan".to_string(),
        };
        let intent = handle_mouse_event(ev, &mode, &mut vs, &areas, 0);
        assert!(intent.is_none());
        assert_eq!(vs.document_review_selected, 0);
    }

    /// Regression: mouse click must select the option at the clicked row, not 1 line above.
    /// dynamic_area.y=18: row 18=header, 19=View, 20=Approve, 21=Refine.
    #[test]
    fn click_document_review_refine_selects_refine_not_approve() {
        let mut vs = ViewState::new();
        let areas = legacy_plan_review_menu_strip_fixture_80x24();
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 21,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let mode = AppMode::DocumentReview {
            content: "plan".to_string(),
        };
        let intent = handle_mouse_event(ev, &mode, &mut vs, &areas, 0);
        assert_eq!(
            vs.document_review_selected, 2,
            "clicking Refine row (21) must select option 2, not option 1 (off-by-one bug)"
        );
        assert!(
            matches!(intent, Some(UserIntent::RefineSessionDocument)),
            "clicking Refine row must produce RefineSessionDocument intent, not ApproveSessionDocument"
        );
    }

    /// Regression: click on View (first option) must select View.
    #[test]
    fn click_document_review_view_selects_view() {
        let mut vs = ViewState::new();
        let areas = legacy_plan_review_menu_strip_fixture_80x24();
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 19,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let mode = AppMode::DocumentReview {
            content: "plan".to_string(),
        };
        let intent = handle_mouse_event(ev, &mode, &mut vs, &areas, 0);
        assert_eq!(vs.document_review_selected, 0);
        assert!(matches!(intent, Some(UserIntent::ViewSessionDocument)));
    }

    /// `event_loop` applies `normalize_mouse_coords_for_local` before `handle_mouse_event`.
    /// Same pattern as DocumentReview: some terminals report the click row one less than the cell row;
    /// raw `dynamic_area.y + 2` becomes `y + 3` after normalize, which is the Continue line.
    #[test]
    fn click_error_recovery_continue_with_agent_after_normalize_matches_event_loop() {
        let areas = areas_from_real_layout_80x24_error_recovery();
        let continue_row_raw = areas.dynamic_area.y + 2;
        let mut vs = ViewState::new();
        let mode = AppMode::ErrorRecovery {
            error_message: "backend timeout".to_string(),
        };
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: continue_row_raw,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let intent = handle_mouse_event(
            normalize_mouse_coords_for_local(ev),
            &mode,
            &mut vs,
            &areas,
            0,
        );
        assert!(
            matches!(intent, Some(UserIntent::ContinueWithAgent)),
            "click Continue row after normalize must produce ContinueWithAgent; got {:?} (selection={})",
            intent,
            vs.error_recovery_selected
        );
    }

    fn select_mode_fixture_80x24() -> (AppMode, LayoutAreas) {
        select_mode_fixture_with_prompt_h(1u16)
    }

    fn select_mode_fixture_with_prompt_h(prompt_h: u16) -> (AppMode, LayoutAreas) {
        let question = tddy_core::backend_selection_question();
        let mode = AppMode::Select {
            question,
            question_index: 0,
            total_questions: 1,
            initial_selected: 0,
        };
        let area = Rect::new(0, 0, 80, 24);
        let dynamic_h = question_height(&mode);
        let (activity_log, _spacer, dynamic_area, status_bar, _gap, _debug, prompt_bar, footer_bar) =
            layout_chunks_with_inbox(area, dynamic_h, 0, prompt_h);
        let areas = LayoutAreas {
            activity_log,
            dynamic_area,
            status_bar,
            prompt_bar,
            footer_bar,
            enter_pane: Rect::default(),
        };
        (mode, areas)
    }

    #[test]
    fn single_click_in_select_mode_highlights_without_confirming() {
        let mut vs = ViewState::new();
        let (mode, areas) = select_mode_fixture_80x24();
        let option_row = areas.dynamic_area.y + 2;
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: option_row,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let intent = handle_mouse_event(ev, &mode, &mut vs, &areas, 0);
        assert_eq!(
            vs.select_selected, 0,
            "single click must highlight option 0"
        );
        assert!(
            intent.is_none(),
            "single click must not confirm selection (no AnswerSelect)"
        );
    }

    #[test]
    fn double_click_in_select_mode_confirms_selection() {
        let mut vs = ViewState::new();
        let (mode, areas) = select_mode_fixture_80x24();
        let option_row = areas.dynamic_area.y + 2;
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: option_row,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let _ = handle_mouse_event(ev, &mode, &mut vs, &areas, 0);
        assert_eq!(vs.select_selected, 0);

        let ev2 = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: option_row,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let intent = handle_mouse_event(ev2, &mode, &mut vs, &areas, 0);
        assert_eq!(
            intent,
            Some(UserIntent::AnswerSelect(0)),
            "double-click (two rapid clicks at same row) must confirm selection"
        );
    }

    #[test]
    fn double_click_on_different_rows_does_not_confirm() {
        let mut vs = ViewState::new();
        let (mode, areas) = select_mode_fixture_80x24();
        let first_option_row = areas.dynamic_area.y + 2;
        let second_option_row = areas.dynamic_area.y + 3;
        let ev1 = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: first_option_row,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let _ = handle_mouse_event(ev1, &mode, &mut vs, &areas, 0);

        let ev2 = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: second_option_row,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let intent = handle_mouse_event(ev2, &mode, &mut vs, &areas, 0);
        assert_eq!(vs.select_selected, 1);
        assert!(
            intent.is_none(),
            "clicking a different row must not confirm — it should only highlight"
        );
    }

    fn rects_overlap(a: Rect, b: Rect) -> bool {
        a.x < b.x.saturating_add(b.width)
            && a.x.saturating_add(a.width) > b.x
            && a.y < b.y.saturating_add(b.height)
            && a.y.saturating_add(a.height) > b.y
    }

    /// Enter strip is three columns wide from the row below the status bar through prompt + footer,
    /// separated from the prompt text block by [`super::ENTER_STRIP_MARGIN_COLS`] and must not overlap
    /// `prompt_bar` cells.
    #[test]
    fn enter_button_rect_is_right_of_prompt_with_margin_and_disjoint_from_prompt_and_footer() {
        let sb = Rect::new(0, 20, 80, 1);
        // One-row gap below status (y=21); debug height 0 so prompt starts at 22.
        let pb = Rect::new(0, 22, 76, 1);
        let fb = Rect::new(0, 23, 76, 1);
        let areas = LayoutAreas {
            activity_log: Rect::new(0, 0, 80, 20),
            dynamic_area: Rect::new(0, 20, 80, 0),
            status_bar: sb,
            prompt_bar: pb,
            footer_bar: fb,
            enter_pane: Rect::default(),
        };
        let r = super::enter_button_rect(&areas);
        let prompt_rows = if pb.height > 1 {
            pb.height - 1
        } else {
            pb.height
        };
        let strip_top = sb.y.saturating_add(sb.height);
        let above_prompt = pb.y.saturating_sub(strip_top);
        let expected_h = above_prompt
            .saturating_add(prompt_rows)
            .saturating_add(fb.height);
        assert_eq!(r.width, super::ENTER_BUTTON_COLS);
        assert_eq!(r.x, pb.x + pb.width + super::ENTER_STRIP_MARGIN_COLS);
        assert_eq!(
            r.y, strip_top,
            "Enter strip must start below status (separator band)"
        );
        assert_eq!(
            r.height, expected_h,
            "Enter height = rows below status through prompt text + footer (exclude prompt rule row); sb={sb:?} pb={pb:?} fb={fb:?} r={r:?}"
        );
        assert!(
            !rects_overlap(r, pb),
            "Enter hit target must not overlap prompt_bar; r={r:?} pb={pb:?}"
        );
        assert!(
            !rects_overlap(r, fb),
            "Enter hit target must not overlap footer_bar; r={r:?} fb={fb:?}"
        );
    }

    #[test]
    fn click_enter_affordance_left_border_confirms_select() {
        let mut vs = ViewState::new();
        let (mode, areas) = select_mode_fixture_80x24();
        vs.select_selected = 0;
        let r = super::enter_button_rect(&areas);
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: r.x,
            row: r.y + 1,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let intent = handle_mouse_event(ev, &mode, &mut vs, &areas, 0);
        assert_eq!(
            intent,
            Some(UserIntent::AnswerSelect(0)),
            "click on vertical border left of the key must act like Enter"
        );
    }

    #[test]
    fn click_enter_affordance_corner_cell_confirms_select() {
        let mut vs = ViewState::new();
        let (mode, areas) = select_mode_fixture_80x24();
        vs.select_selected = 0;
        let r = super::enter_button_rect(&areas);
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: r.x,
            row: r.y,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let intent = handle_mouse_event(ev, &mode, &mut vs, &areas, 0);
        assert_eq!(
            intent,
            Some(UserIntent::AnswerSelect(0)),
            "click on top-left corner of the ASCII frame must act like Enter"
        );
    }

    #[test]
    fn click_enter_affordance_return_symbol_cell_confirms_select() {
        let mut vs = ViewState::new();
        let (mode, areas) = select_mode_fixture_80x24();
        vs.select_selected = 0;
        let r = super::enter_button_rect(&areas);
        let above = areas
            .prompt_bar
            .y
            .saturating_sub(areas.status_bar.y.saturating_add(areas.status_bar.height));
        let return_row = if r.height <= 1 {
            r.y
        } else if above >= 1 {
            r.y.saturating_add(above)
        } else if r.height >= 3 {
            r.y + 1
        } else {
            r.y
        };
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: r.x + 1,
            row: return_row,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let intent = handle_mouse_event(ev, &mode, &mut vs, &areas, 0);
        assert_eq!(
            intent,
            Some(UserIntent::AnswerSelect(0)),
            "click on the U+23CE key cell must confirm the selected option"
        );
    }

    /// PRD: Approve / Refine sit at the bottom of the activity rect (not the old PlanReview strip).
    /// With a normal layout, the footer is two lines; Approve is the upper line, Reject the lower.
    #[test]
    fn plan_approval_footer_buttons_in_activity_hit_test() {
        let mut vs = ViewState::new();
        let area = Rect::new(0, 0, 80, 24);
        let dynamic_h = 0u16;
        let prompt_h = 1u16;
        let (activity_log, _spacer, dynamic_area, status_bar, _gap, _debug, prompt_bar, footer_bar) =
            layout_chunks_with_inbox(area, dynamic_h, 0, prompt_h);
        let areas = LayoutAreas {
            activity_log,
            dynamic_area,
            status_bar,
            prompt_bar,
            footer_bar,
            enter_pane: Rect::default(),
        };
        let mode = AppMode::MarkdownViewer {
            content: "# Plan".to_string(),
        };
        let h = activity_log.height;
        let footer_h = if h >= 2 { 2 } else { 1 }.min(h);
        let footer_top = activity_log.y + h - footer_h;
        let approve_row = footer_top;
        let reject_row = footer_top + footer_h.saturating_sub(1);
        let ev_approve = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: approve_row,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let intent_approve = handle_mouse_event(ev_approve, &mode, &mut vs, &areas, 0);
        assert_eq!(
            intent_approve,
            Some(UserIntent::ApproveSessionDocument),
            "click on Approve footer line must approve"
        );
        let ev_reject = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: reject_row,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let intent_reject = handle_mouse_event(ev_reject, &mode, &mut vs, &areas, 0);
        assert_eq!(
            intent_reject,
            Some(UserIntent::RefineSessionDocument),
            "click on Reject footer line must request refinement"
        );
    }
}
