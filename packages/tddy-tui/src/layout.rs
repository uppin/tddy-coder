//! Layout computation for the TUI: activity log, status bar, prompt bar, inbox.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

use tddy_core::AppMode;

/// Compute the height (in lines) for the question display region.
/// Used when in Select, MultiSelect, TextInput, or DocumentReview mode.
pub fn question_height(mode: &AppMode) -> u16 {
    match mode {
        AppMode::Select { question, .. } | AppMode::MultiSelect { question, .. } => {
            // header(1) + question(1) + options + Other(1) when allow_other
            2 + question.options.len() as u16 + if question.allow_other { 1 } else { 0 }
        }
        AppMode::TextInput { .. } => 2, // prompt + blank
        // DocumentReview menu is in the activity pane; no extra strip in the dynamic/question region.
        AppMode::DocumentReview { .. } => 0,
        AppMode::ErrorRecovery { .. } => 5, // error + blank + 3 options
        _ => 0,
    }
}

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

/// Vertical size of the prompt **chunk** in the layout: wrapped text lines (at the content width
/// that excludes the Enter strip and margin) plus one row for the bottom horizontal rule (`U+2500`).
pub fn prompt_chunk_height_including_rule(
    text_len: u16,
    terminal_width: u16,
    terminal_height: u16,
) -> u16 {
    let reserve = crate::mouse_map::right_chrome_reserve_cols(terminal_width);
    let content_width = terminal_width.saturating_sub(reserve).max(1);
    let max_height = (terminal_height / 3).max(1);
    let text_lines = prompt_height(text_len, content_width, max_height.saturating_sub(1).max(1));
    text_lines.saturating_add(1)
}

/// Compute the number of terminal lines needed to display `text_len` characters
/// at `area_width` columns wide, capped at `max_height`.
///
/// Returns at least 1. Returns 1 when `area_width` is 0 (safe edge case).
pub fn prompt_height(text_len: u16, area_width: u16, max_height: u16) -> u16 {
    log::trace!(
        "prompt_height: text_len={text_len} area_width={area_width} max_height={max_height}"
    );
    if area_width == 0 || text_len == 0 {
        return 1;
    }
    // Ceiling division: how many lines of area_width fit text_len chars
    let lines = text_len.div_ceil(area_width);
    let result = lines.max(1).min(max_height);
    log::trace!("prompt_height -> {result}");
    result
}

/// Whether clarification / text-input prompts should be pinned to the **top** of the terminal.
///
/// Default layout places the dynamic strip between the activity log and the status bar; long agent
/// output makes that easy to miss. For interactive elicitation we put questions first so they stay
/// visible.
pub fn clarification_questions_top(mode: &AppMode) -> bool {
    matches!(
        mode,
        AppMode::Select { .. } | AppMode::MultiSelect { .. } | AppMode::TextInput { .. }
    ) && question_height(mode) > 0
}

/// Split the terminal area into **eight** regions: activity, spacer, dynamic (inbox), status,
/// **empty row** (gap between status and prompt), debug log, prompt, and **footer** (PRD: exactly one
/// footer row below the prompt block).
pub fn layout_chunks_with_inbox(
    area: Rect,
    inbox_h: u16,
    debug_log_h: u16,
    prompt_h: u16,
) -> (Rect, Rect, Rect, Rect, Rect, Rect, Rect, Rect) {
    crate::red_phase::tddy_marker("M001", "layout::layout_chunks_with_inbox");
    log::debug!(
        "layout_chunks_with_inbox: area={area:?} inbox_h={inbox_h} debug_log_h={debug_log_h} prompt_h={prompt_h} footer_h=1"
    );
    log::trace!("layout_chunks_with_inbox: area={area:?} inbox_h={inbox_h} debug_log_h={debug_log_h} prompt_h={prompt_h}");
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(inbox_h),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(debug_log_h),
            Constraint::Length(prompt_h),
            Constraint::Length(1),
        ])
        .split(area);
    let reserve = crate::mouse_map::right_chrome_reserve_cols(area.width);
    let inner_w = area.width.saturating_sub(reserve);
    let mut prompt_bar = chunks[6];
    let mut footer_bar = chunks[7];
    prompt_bar.width = inner_w.min(prompt_bar.width);
    footer_bar.width = inner_w.min(footer_bar.width);
    log::trace!(
        "layout_chunks_with_inbox: activity={:?} footer_bar={:?}",
        chunks[0],
        footer_bar
    );
    (
        chunks[0], chunks[1], chunks[2], chunks[3], chunks[4], chunks[5], prompt_bar, footer_bar,
    )
}

/// Same regions as [`layout_chunks_with_inbox`], but when `questions_top` is true and
/// `dynamic_h > 0`, the dynamic strip (questions / inbox / slash menu height) is placed at the
/// **top** of the frame, above the activity log.
pub fn layout_chunks_with_inbox_maybe_top(
    area: Rect,
    dynamic_h: u16,
    debug_log_h: u16,
    prompt_h: u16,
    questions_top: bool,
) -> (Rect, Rect, Rect, Rect, Rect, Rect, Rect, Rect) {
    if !questions_top || dynamic_h == 0 {
        return layout_chunks_with_inbox(area, dynamic_h, debug_log_h, prompt_h);
    }
    log::trace!(
        "layout_chunks_with_inbox_maybe_top: area={area:?} dynamic_h={dynamic_h} debug_log_h={debug_log_h} prompt_h={prompt_h}"
    );
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(dynamic_h),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(debug_log_h),
            Constraint::Length(prompt_h),
            Constraint::Length(1),
        ])
        .split(area);
    let reserve = crate::mouse_map::right_chrome_reserve_cols(area.width);
    let inner_w = area.width.saturating_sub(reserve);
    let mut prompt_bar = chunks[6];
    let mut footer_bar = chunks[7];
    prompt_bar.width = inner_w.min(prompt_bar.width);
    footer_bar.width = inner_w.min(footer_bar.width);
    (
        chunks[2], // activity_log
        chunks[1], // spacer
        chunks[0], // dynamic_area
        chunks[3], // status_bar
        chunks[4], // gap between status and prompt region
        chunks[5], // debug_log
        prompt_bar, footer_bar,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use tddy_core::{ClarificationQuestion, QuestionOption};

    #[test]
    fn question_height_counts_header_question_options_and_other_in_select_mode() {
        // Given — two options + allow_other = 5 rows total (header + question + 2 opts + Other)
        let q = ClarificationQuestion {
            header: "Scope".to_string(),
            question: "Which authentication?".to_string(),
            options: vec![
                QuestionOption {
                    label: "A".to_string(),
                    description: "opt A".to_string(),
                },
                QuestionOption {
                    label: "B".to_string(),
                    description: "opt B".to_string(),
                },
            ],
            multi_select: false,
            allow_other: true,
        };
        let mode = AppMode::Select {
            question: q,
            question_index: 0,
            total_questions: 1,
            initial_selected: 0,
        };

        // When / Then
        assert_eq!(question_height(&mode), 5, "2 (header+question) + 2 options + 1 Other");
    }

    #[test]
    fn question_height_returns_two_for_text_input_mode() {
        // Given
        let mode = AppMode::TextInput {
            prompt: "Type your answer".to_string(),
        };

        // When / Then
        assert_eq!(question_height(&mode), 2);
    }

    #[test]
    fn question_height_returns_zero_for_running_mode() {
        // When / Then
        assert_eq!(question_height(&AppMode::Running), 0);
    }


    #[test]
    fn layout_chunks_returns_four_non_zero_regions() {
        // Given
        let area = Rect::new(0, 0, 80, 24);

        // When
        let (activity, spacer, status, prompt) = layout_chunks(area);

        // Then
        assert!(activity.width > 0, "activity region must have non-zero width");
        assert!(activity.height > 0, "activity region must have non-zero height");
        assert_eq!(spacer.height, 1, "spacer is one row");
        assert_eq!(status.height, 1, "status bar is one row");
        assert!(prompt.height >= 1, "prompt must have at least one row");
    }

    #[rstest]
    #[case::zero_items_visible(0, true, 0)]
    #[case::items_present_but_not_visible(3, false, 0)]
    #[case::one_item_visible(1, true, 1)]
    #[case::ten_items_capped_at_five(10, true, 5)]
    fn inbox_height_returns_zero_when_empty_or_not_visible(
        #[case] count: usize,
        #[case] visible: bool,
        #[case] expected: u16,
    ) {
        // When / Then
        assert_eq!(inbox_height(count, visible), expected);
    }

    #[rstest]
    #[case::fits_in_one_line(40, 80, 10, 1)]
    #[case::exactly_fills_width(80, 80, 10, 1)]
    #[case::exceeds_width_by_one(81, 80, 10, 2)]
    #[case::triple_width_text(240, 80, 10, 3)]
    #[case::capped_at_max(1000, 80, 5, 5)]
    #[case::zero_width(50, 0, 10, 1)]
    #[case::empty_text(0, 80, 10, 1)]
    fn prompt_height_wraps_correctly(
        #[case] text_len: u16,
        #[case] width: u16,
        #[case] max_h: u16,
        #[case] expected: u16,
    ) {
        // When / Then
        assert_eq!(prompt_height(text_len, width, max_h), expected);
    }

    #[test]
    fn question_height_returns_five_for_error_recovery_mode() {
        // Given
        let mode = AppMode::ErrorRecovery {
            error_message: "test error".to_string(),
        };

        // When / Then
        assert_eq!(
            question_height(&mode),
            5,
            "ErrorRecovery: error message row + blank + 3 option rows"
        );
    }

    #[test]
    fn layout_chunks_prompt_bar_height_matches_given_parameter() {
        // Given
        let area = Rect::new(0, 0, 80, 24);
        let prompt_h: u16 = 3;

        // When
        let (_activity, _spacer, _dynamic, _status, _gap, _debug, prompt, _footer) =
            layout_chunks_with_inbox(area, 0, 0, prompt_h);

        // Then
        assert_eq!(prompt.height, prompt_h, "prompt bar height must match the given parameter");
    }

    /// PRD (plan approval activity pane): PRD + footer live in the activity region; the dynamic
    /// strip below activity must not reserve extra rows for the old PlanReview three-option menu.
    #[test]
    fn layout_reserves_status_and_prompt_when_plan_approval_visible() {
        let mode = AppMode::DocumentReview {
            content: "# PRD".to_string(),
        };
        assert_eq!(
            question_height(&mode),
            0,
            "plan approval surface is activity-only: question strip height must be 0 (no View/Approve/Refine menu block)"
        );
        let area = Rect::new(0, 0, 80, 24);
        let dynamic_h = question_height(&mode).max(inbox_height(0, false));
        let (activity, spacer, dynamic, status, _gap, _debug, prompt, _footer) =
            layout_chunks_with_inbox(area, dynamic_h, 0, 1);
        assert_eq!(spacer.height, 1, "status spacer row");
        assert_eq!(
            dynamic.height, 0,
            "no separate dynamic strip for plan approval menu"
        );
        assert_eq!(status.height, 1, "status bar is one row");
        assert_eq!(prompt.height, 1, "prompt bar is one line in this fixture");
        assert_eq!(activity.y, 0);
        assert_eq!(
            status.y,
            activity.y + activity.height + spacer.height + dynamic.height,
            "status must sit below activity, not inside it"
        );
        assert_eq!(
            prompt.y,
            status.y + status.height + 1,
            "prompt must be below status and the one-row gap"
        );
        assert!(activity.height > 0, "non-zero activity on 24×80");
    }

    #[test]
    fn clarification_questions_top_is_true_for_select_with_height() {
        // Given
        let q = ClarificationQuestion {
            header: "H".to_string(),
            question: "Q?".to_string(),
            options: vec![QuestionOption {
                label: "a".to_string(),
                description: "".to_string(),
            }],
            multi_select: false,
            allow_other: false,
        };
        let mode = AppMode::Select {
            question: q,
            question_index: 0,
            total_questions: 1,
            initial_selected: 0,
        };

        // When / Then
        assert!(clarification_questions_top(&mode));
    }

    #[test]
    fn questions_top_layout_places_dynamic_strip_at_top() {
        // Given
        let area = Rect::new(0, 0, 80, 24);
        let dynamic_h = 7u16;

        // When
        let (activity, _spacer, dynamic, status, _gap, _debug, prompt, _footer) =
            layout_chunks_with_inbox_maybe_top(area, dynamic_h, 0, 1, true);

        // Then
        assert_eq!(dynamic.y, 0);
        assert!(
            activity.y > dynamic.y,
            "activity log must sit below the elicitation strip"
        );
        assert_eq!(dynamic.height, dynamic_h);
        assert_eq!(status.height, 1);
        assert_eq!(prompt.height, 1);
    }

    /// PRD AC2: footer adds exactly one row to bottom chrome (status + prompt + **footer**).
    #[test]
    fn layout_footer_adds_exactly_one_row_to_bottom_chrome() {
        // Given
        let area = Rect::new(0, 0, 80, 24);

        // When
        let (_, _, _, status, _, _, prompt, footer) = layout_chunks_with_inbox(area, 0, 0, 1);

        // Then
        assert_eq!(
            footer.height,
            1,
            "layout must allocate exactly one footer row below the prompt block (PRD: +1 row contract)"
        );
        assert_eq!(
            status.height + prompt.height + footer.height,
            status.height + prompt.height + 1,
            "bottom chrome row count must include the single footer row"
        );
    }

    /// Prompt row width leaves right chrome (Enter + Stop when wide enough).
    #[test]
    fn prompt_bar_width_reserves_margin_and_enter_columns() {
        use crate::mouse_map::right_chrome_reserve_cols;

        // Given
        let area = Rect::new(0, 0, 80, 24);

        // When
        let (_, _, _, _, _, _, prompt_bar, _) = layout_chunks_with_inbox(area, 0, 0, 1);

        // Then
        let expected = area
            .width
            .saturating_sub(right_chrome_reserve_cols(area.width));
        assert_eq!(prompt_bar.width, expected);
    }

    /// Narrow terminal: only Enter strip reserved (no Stop).
    #[test]
    fn prompt_bar_width_enter_only_when_too_narrow_for_stop() {
        use crate::mouse_map::{right_chrome_reserve_cols, ENTER_RESERVE_COLS};

        // Given
        let area = Rect::new(0, 0, 8, 24);

        // When
        let (_, _, _, _, _, _, prompt_bar, _) = layout_chunks_with_inbox(area, 0, 0, 1);

        // Then
        assert_eq!(right_chrome_reserve_cols(area.width), ENTER_RESERVE_COLS);
        let expected = area.width.saturating_sub(ENTER_RESERVE_COLS);
        assert_eq!(prompt_bar.width, expected);
    }
}
