//! Layout computation for the TUI: activity log, status bar, prompt bar, inbox.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

use tddy_core::AppMode;

/// Compute the height (in lines) for the question display region.
/// Used when in Select, MultiSelect, TextInput, or PlanReview mode.
pub fn question_height(mode: &AppMode) -> u16 {
    match mode {
        AppMode::Select { question, .. } | AppMode::MultiSelect { question, .. } => {
            // header(1) + question(1) + options + Other(1) when allow_other
            2 + question.options.len() as u16 + if question.allow_other { 1 } else { 0 }
        }
        AppMode::TextInput { .. } => 2,     // prompt + blank
        AppMode::PlanReview { .. } => 4,    // header + 3 options (View, Approve, Refine)
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

/// Split the terminal area into six regions, including inbox and optional debug log.
pub fn layout_chunks_with_inbox(
    area: Rect,
    inbox_h: u16,
    debug_log_h: u16,
    prompt_h: u16,
) -> (Rect, Rect, Rect, Rect, Rect, Rect) {
    log::trace!("layout_chunks_with_inbox: area={area:?} inbox_h={inbox_h} debug_log_h={debug_log_h} prompt_h={prompt_h}");
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(inbox_h),
            Constraint::Length(1),
            Constraint::Length(debug_log_h),
            Constraint::Length(prompt_h),
        ])
        .split(area);
    (
        chunks[0], chunks[1], chunks[2], chunks[3], chunks[4], chunks[5],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_core::{ClarificationQuestion, QuestionOption};

    #[test]
    fn test_question_height_select_mode() {
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
        };
        assert_eq!(question_height(&mode), 5); // 2 (header+question) + 2 options + 1 Other
    }

    #[test]
    fn test_question_height_text_input_mode() {
        let mode = AppMode::TextInput {
            prompt: "Type your answer".to_string(),
        };
        assert_eq!(question_height(&mode), 2);
    }

    #[test]
    fn test_question_height_running_mode() {
        let mode = AppMode::Running;
        assert_eq!(question_height(&mode), 0);
    }

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

    // Acceptance tests for prompt_height (AC1–AC5, FR2)

    #[test]
    fn test_prompt_height_single_line() {
        assert_eq!(prompt_height(40, 80, 10), 1);
    }

    #[test]
    fn test_prompt_height_exact_width() {
        assert_eq!(prompt_height(80, 80, 10), 1);
    }

    #[test]
    fn test_prompt_height_wraps_to_two_lines() {
        assert_eq!(prompt_height(81, 80, 10), 2);
    }

    #[test]
    fn test_prompt_height_wraps_to_three_lines() {
        assert_eq!(prompt_height(240, 80, 10), 3);
    }

    #[test]
    fn test_prompt_height_capped_at_max() {
        assert_eq!(prompt_height(1000, 80, 5), 5);
    }

    #[test]
    fn test_prompt_height_zero_width_edge_case() {
        assert_eq!(prompt_height(50, 0, 10), 1);
    }

    #[test]
    fn test_prompt_height_empty_text() {
        assert_eq!(prompt_height(0, 80, 10), 1);
    }

    #[test]
    fn test_question_height_error_recovery_mode() {
        let mode = AppMode::ErrorRecovery {
            error_message: "test error".to_string(),
        };
        assert_eq!(
            question_height(&mode),
            5,
            "ErrorRecovery should have height 5: error + blank + 3 options"
        );
    }

    #[test]
    fn test_layout_prompt_bar_height_matches_param() {
        let area = Rect::new(0, 0, 80, 24);
        let prompt_h: u16 = 3;
        let (_activity, _spacer, _status, _inbox, _debug, prompt) =
            layout_chunks_with_inbox(area, 0, 0, prompt_h);
        assert_eq!(prompt.height, prompt_h);
    }
}
