//! Frame rendering: draw activity log, status bar, prompt bar.

use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use tddy_core::{ActivityEntry, AppMode, PresenterState};

use crate::layout::{
    debug_log_height, inbox_height, layout_chunks_with_inbox, prompt_height, question_height,
};
use crate::ui::{format_status_bar, status_bar_style_for_goal};
use crate::view_state::{InboxFocus, ViewState};

/// Return the prompt bar text for the current mode and view state.
fn prompt_text(state: &PresenterState, view_state: &ViewState) -> String {
    match &state.mode {
        AppMode::FeatureInput => {
            let prefix = "> ";
            if view_state.feature_input.is_empty() {
                format!("{}Type your feature description and press Enter...", prefix)
            } else {
                format!("{}{}", prefix, view_state.feature_input)
            }
        }
        AppMode::Running => {
            if view_state.running_input.is_empty() {
                "> Queue a follow-up prompt...".to_string()
            } else {
                format!("> {}", view_state.running_input)
            }
        }
        AppMode::Done => "> Workflow complete. Press Enter to exit.".to_string(),
        AppMode::Select { .. } => {
            if view_state.select_typing_other {
                if view_state.select_other_text.is_empty() {
                    "> Type your answer and press Enter...".to_string()
                } else {
                    format!("> {}", view_state.select_other_text)
                }
            } else {
                "Up/Down navigate  Enter select".to_string()
            }
        }
        AppMode::MultiSelect { .. } => {
            if view_state.multiselect_typing_other {
                if view_state.multiselect_other_text.is_empty() {
                    "> Type your answer and press Enter...".to_string()
                } else {
                    format!("> {}", view_state.multiselect_other_text)
                }
            } else {
                "Up/Down navigate  Space toggle  Enter submit".to_string()
            }
        }
        AppMode::TextInput { .. } => {
            if view_state.text_input.is_empty() {
                "> Type your answer and press Enter...".to_string()
            } else {
                format!("> {}", view_state.text_input)
            }
        }
        AppMode::PlanReview { .. } => "Up/Down navigate  Enter select".to_string(),
        AppMode::MarkdownViewer { .. } => "Q or Esc to close".to_string(),
        AppMode::ErrorRecovery { .. } => "Up/Down navigate  Enter select".to_string(),
    }
}

/// Draw the TUI layout: activity log, status bar, prompt bar.
/// When `debug` is true, the debug log area is shown; otherwise it is hidden.
pub fn draw(frame: &mut Frame, state: &PresenterState, view_state: &ViewState, debug: bool) {
    let is_running = matches!(state.mode, AppMode::Running);
    let inbox_h = inbox_height(state.inbox.len(), is_running);
    let question_h = question_height(&state.mode);
    let dynamic_h = question_h.max(inbox_h);
    let debug_logs = if debug {
        tddy_core::get_buffered_logs()
    } else {
        vec![]
    };
    let debug_h = debug_log_height(debug_logs.len());

    let area = frame.area();
    let prompt_text_str = prompt_text(state, view_state);
    let text_len = prompt_text_str.chars().count().min(u16::MAX as usize) as u16;
    let area_width = area.width;
    let max_height = (area.height / 3).max(1);
    let prompt_h = prompt_height(text_len, area_width, max_height);

    let (activity_log, _status_spacer, dynamic_area, status_bar, debug_log, prompt_bar) =
        layout_chunks_with_inbox(area, dynamic_h, debug_h, prompt_h);

    if activity_log.height > 0 {
        match &state.mode {
            AppMode::MarkdownViewer { content } => {
                let text = tui_markdown::from_str(content);
                let widget =
                    Paragraph::new(text).scroll((view_state.markdown_scroll_offset as u16, 0));
                frame.render_widget(widget, activity_log);
            }
            _ => {
                let content = state
                    .activity_log
                    .iter()
                    .map(|e: &ActivityEntry| e.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                let line_count = state.activity_log.len();
                let area_height = activity_log.height as usize;
                let scroll_y = if line_count > area_height {
                    let max_scroll = line_count.saturating_sub(area_height);
                    (view_state.scroll_offset.min(max_scroll)) as u16
                } else {
                    0
                };
                let widget = Paragraph::new(content).scroll((scroll_y, 0));
                frame.render_widget(widget, activity_log);
            }
        }
    }

    if dynamic_area.height > 0 {
        match &state.mode {
            AppMode::Select { .. } | AppMode::MultiSelect { .. } | AppMode::TextInput { .. } => {
                render_question(frame, state, view_state, dynamic_area);
            }
            AppMode::PlanReview { .. } => {
                render_plan_review(frame, state, view_state, dynamic_area);
            }
            _ => {
                render_inbox(frame, state, view_state, dynamic_area);
            }
        }
    }

    if debug_log.height > 0 && !debug_logs.is_empty() {
        use ratatui::style::Style;
        let content = debug_logs
            .iter()
            .rev()
            .take(debug_log.height as usize)
            .rev()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let widget =
            Paragraph::new(content).style(Style::default().fg(ratatui::style::Color::DarkGray));
        frame.render_widget(widget, debug_log);
    }

    if status_bar.height > 0 {
        let text = match (&state.current_goal, &state.current_state) {
            (Some(goal), Some(s)) => {
                let elapsed = state.goal_start_time.elapsed();
                format_status_bar(goal, s, elapsed, &state.agent, &state.model)
            }
            _ => format!(
                "Goal: — │ State: — │ Ready │ {} {} │ PgUp/PgDn scroll",
                state.agent, state.model
            ),
        };
        let style = status_bar_style_for_goal(state.current_goal.as_deref());
        let widget = Paragraph::new(text).style(style);
        frame.render_widget(widget, status_bar);
    }

    if prompt_bar.height > 0 {
        let widget = Paragraph::new(prompt_text_str.as_str()).wrap(Wrap { trim: false });
        frame.render_widget(widget, prompt_bar);
    }
}

/// Render clarification question (Select, MultiSelect, or TextInput mode).
fn render_question(
    frame: &mut Frame,
    state: &PresenterState,
    view_state: &ViewState,
    area: ratatui::layout::Rect,
) {
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};

    if area.height == 0 {
        return;
    }

    let lines: Vec<Line> = match &state.mode {
        AppMode::Select {
            question,
            question_index,
            total_questions: _,
        } => {
            let header = format!(
                "[{}] {}: {}",
                question_index + 1,
                question.header,
                question.question
            );
            let mut result = vec![Line::from(Span::styled(
                header,
                Style::default().add_modifier(Modifier::BOLD),
            ))];
            let other_idx = question.options.len();
            for (i, opt) in question.options.iter().enumerate() {
                let prefix = if view_state.select_selected == i {
                    "> "
                } else {
                    "  "
                };
                let text = format!("{}{} -- {}", prefix, opt.label, opt.description);
                let style = if view_state.select_selected == i {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                result.push(Line::from(Span::styled(text, style)));
            }
            if question.allow_other {
                let other_prefix = if view_state.select_selected == other_idx {
                    "> "
                } else {
                    "  "
                };
                let other_text = if view_state.select_typing_other {
                    format!("{}Other: {}", other_prefix, view_state.select_other_text)
                } else {
                    format!("{}Other (type your own)", other_prefix)
                };
                let other_style = if view_state.select_selected == other_idx {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                result.push(Line::from(Span::styled(other_text, other_style)));
            }
            result
        }
        AppMode::MultiSelect {
            question,
            question_index,
            total_questions: _,
        } => {
            let header = format!(
                "[{}] {}: {}",
                question_index + 1,
                question.header,
                question.question
            );
            let mut result = vec![Line::from(Span::styled(
                header,
                Style::default().add_modifier(Modifier::BOLD),
            ))];
            let other_idx = question.options.len();
            for (i, opt) in question.options.iter().enumerate() {
                let checked = view_state
                    .multiselect_checked
                    .get(i)
                    .copied()
                    .unwrap_or(false);
                let checkbox = if checked { "[x]" } else { "[ ]" };
                let prefix = if view_state.multiselect_cursor == i {
                    "> "
                } else {
                    "  "
                };
                let text = format!(
                    "{}{} {} -- {}",
                    prefix, checkbox, opt.label, opt.description
                );
                let style = if view_state.multiselect_cursor == i {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                result.push(Line::from(Span::styled(text, style)));
            }
            if question.allow_other {
                let other_checked = view_state
                    .multiselect_checked
                    .get(other_idx)
                    .copied()
                    .unwrap_or(false);
                let other_cb = if other_checked { "[x]" } else { "[ ]" };
                let other_prefix = if view_state.multiselect_cursor == other_idx {
                    "> "
                } else {
                    "  "
                };
                let other_text = if view_state.multiselect_typing_other {
                    format!(
                        "{}{} Other: {}",
                        other_prefix, other_cb, view_state.multiselect_other_text
                    )
                } else {
                    format!("{}{} Other (type your own)", other_prefix, other_cb)
                };
                let other_style = if view_state.multiselect_cursor == other_idx {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                result.push(Line::from(Span::styled(other_text, other_style)));
            }
            result
        }
        AppMode::TextInput { prompt } => {
            vec![
                Line::from(Span::styled(
                    prompt.as_str(),
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::raw("")),
            ]
        }
        _ => return,
    };

    let widget = Paragraph::new(lines);
    frame.render_widget(widget, area);
}

/// Render plan approval 3-option menu
fn render_plan_review(
    frame: &mut Frame,
    _state: &PresenterState,
    view_state: &ViewState,
    area: ratatui::layout::Rect,
) {
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};

    if area.height == 0 {
        return;
    }

    let options = [
        ("View", "Open full-screen PRD viewer"),
        ("Approve", "Proceed to next step"),
        ("Refine", "Enter feedback for plan refinement"),
    ];
    let mut lines = vec![Line::from(Span::styled(
        "Plan generated. Choose an action:",
        Style::default().add_modifier(Modifier::BOLD),
    ))];
    for (i, (label, desc)) in options.iter().enumerate() {
        let prefix = if view_state.plan_review_selected == i {
            "> "
        } else {
            "  "
        };
        let text = format!("{}{} -- {}", prefix, label, desc);
        let style = if view_state.plan_review_selected == i {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    let widget = Paragraph::new(lines);
    frame.render_widget(widget, area);
}

/// Render inbox items.
fn render_inbox(
    frame: &mut Frame,
    state: &PresenterState,
    view_state: &ViewState,
    area: ratatui::layout::Rect,
) {
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};

    if area.height == 0 || state.inbox.is_empty() {
        return;
    }

    let highlight_style = Style::default().add_modifier(Modifier::REVERSED);
    let normal_style = Style::default();

    let lines: Vec<Line> = state
        .inbox
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_selected = matches!(
                view_state.inbox_focus,
                InboxFocus::List | InboxFocus::Editing
            ) && i == view_state.inbox_cursor;
            let display_text = if is_selected && view_state.inbox_focus == InboxFocus::Editing {
                &view_state.inbox_edit_buffer
            } else {
                item
            };
            let text = format!("[{}] {}", i + 1, display_text);
            let style = if is_selected {
                highlight_style
            } else {
                normal_style
            };
            Line::from(Span::styled(text, style))
        })
        .collect();

    let widget = Paragraph::new(lines);
    frame.render_widget(widget, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use tddy_core::{AppMode, PresenterState};

    fn make_state(mode: AppMode) -> PresenterState {
        PresenterState {
            agent: "test-agent".to_string(),
            model: "test-model".to_string(),
            mode,
            current_goal: None,
            current_state: None,
            goal_start_time: Instant::now(),
            activity_log: Vec::new(),
            inbox: Vec::new(),
            should_quit: false,
        }
    }

    #[test]
    fn test_error_recovery_prompt_text() {
        let state = make_state(AppMode::ErrorRecovery {
            error_message: "timeout".to_string(),
        });
        let vs = ViewState::new();
        let text = prompt_text(&state, &vs);
        assert_eq!(text, "Up/Down navigate  Enter select");
    }
}
