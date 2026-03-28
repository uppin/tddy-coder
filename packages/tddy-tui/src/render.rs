//! Frame rendering: draw activity log, status bar, prompt bar.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use tddy_core::{ActivityEntry, AppMode, PresenterState};

use crate::layout::{
    debug_log_height, inbox_height, layout_chunks_with_inbox, prompt_height, question_height,
};
use crate::mouse_map::LayoutAreas;
use crate::status_bar_activity::{
    activity_prefix_char_for_draw, display_elapsed_for_goal_row, status_activity_is_agent_active,
};
use crate::ui::{
    first_hyphen_segment_of_workflow_session_id, format_status_bar_idle,
    format_status_bar_with_activity_prefix, prepend_activity_to_status_line,
    status_bar_style_for_goal,
};
use crate::view_state::{InboxFocus, ViewState};

/// Wrapped line count for markdown [`Text`] at a given width (used for scroll bounds and end-of-doc).
fn markdown_wrapped_line_count(text: &Text, width: u16) -> usize {
    let w = width as usize;
    if w == 0 {
        return text.lines.len().max(1);
    }
    let mut total = 0usize;
    for line in &text.lines {
        let lw = line.width();
        total += lw.div_ceil(w).max(1);
    }
    total.max(1)
}

fn markdown_viewer_prompt_for_plan_approval(
    state: &PresenterState,
    view_state: &ViewState,
) -> String {
    log::debug!(
        "markdown_viewer_prompt: pending={} markdown_end_button_selected={}",
        state.plan_refinement_pending,
        view_state.markdown_end_button_selected
    );
    if state.plan_refinement_pending {
        if view_state.plan_refinement_input.is_empty() {
            "> Type refinement feedback and press Enter...".to_string()
        } else {
            format!("> {}", view_state.plan_refinement_input)
        }
    } else if !view_state.plan_refinement_input.is_empty() {
        format!("> {}", view_state.plan_refinement_input)
    } else if view_state.markdown_at_end && view_state.markdown_end_button_selected == 1 {
        "Type refinement feedback in the prompt below, then press Enter  |  Q/Esc to close"
            .to_string()
    } else {
        "Q or Esc to close  |  Alt+A=Approve  Alt+R=Reject/refine  |  PgUp/PgDn scroll".to_string()
    }
}

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
        AppMode::DocumentReview { .. } => "Up/Down navigate  Enter select".to_string(),
        AppMode::MarkdownViewer { .. } => {
            markdown_viewer_prompt_for_plan_approval(state, view_state)
        }
        AppMode::ErrorRecovery { .. } => "Up/Down navigate  Enter select".to_string(),
    }
}

/// Draw the TUI layout: activity log, status bar, prompt bar.
/// When `debug` is true, the debug log area is shown; otherwise it is hidden.
const SPINNER_FRAMES: &[char] = &['|', '/', '-', '\\'];

/// Builds the single-line status bar text for the primary and Virtual TUI [`draw`] path.
///
/// Leading content is the cycling spinner (agent-active) or 1 Hz idle dot (clarification wait), then
/// the workflow session segment and `Goal:` … tail.
fn status_bar_text_for_draw(state: &PresenterState, view_state: &mut ViewState) -> String {
    view_state.sync_status_bar_with_presenter(state);
    let agent_active = status_activity_is_agent_active(&state.mode);
    let spinner_char = activity_prefix_char_for_draw(&state.mode, view_state, SPINNER_FRAMES);
    log::trace!(
        "status_bar_text_for_draw: agent_active={} tick={} prefix={:?} workflow_session_id={:?}",
        agent_active,
        view_state.spinner_tick,
        spinner_char,
        state
            .workflow_session_id
            .as_deref()
            .map(truncate_session_id_for_log)
    );
    let segment = first_hyphen_segment_of_workflow_session_id(state.workflow_session_id.as_deref());
    let segment_str = segment.as_ref();
    match (&state.current_goal, &state.current_state) {
        (Some(goal), Some(s)) => {
            let elapsed = display_elapsed_for_goal_row(state, view_state);
            format_status_bar_with_activity_prefix(
                spinner_char,
                segment_str,
                goal,
                s,
                elapsed,
                &state.agent,
                &state.model,
            )
        }
        _ => {
            let idle_tail = format_status_bar_idle(&state.agent, &state.model);
            prepend_activity_to_status_line(spinner_char, segment_str, &idle_tail)
        }
    }
}

/// Truncate long session ids in trace logs (correlation id, not a secret, but avoid full dumps).
fn truncate_session_id_for_log(id: &str) -> String {
    const MAX: usize = 12;
    if id.len() <= MAX {
        id.to_string()
    } else {
        format!("{}…", &id[..MAX])
    }
}

/// Draw the TUI. When `layout_areas` is Some, stores the layout rects for mouse hit-testing.
pub fn draw(
    frame: &mut Frame,
    state: &PresenterState,
    view_state: &mut ViewState,
    debug: bool,
    layout_areas: Option<&mut LayoutAreas>,
) {
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

    if let Some(areas) = layout_areas {
        *areas = LayoutAreas {
            activity_log,
            dynamic_area,
            status_bar,
            prompt_bar,
        };
    }

    if activity_log.height > 0 {
        match &state.mode {
            AppMode::DocumentReview { content } => {
                const MENU_LINES: u16 = 4;
                let menu_h = MENU_LINES.min(activity_log.height);
                let body_h = activity_log.height.saturating_sub(menu_h);
                if body_h > 0 {
                    let body_area =
                        Rect::new(activity_log.x, activity_log.y, activity_log.width, body_h);
                    let text = tui_markdown::from_str(content);
                    let widget = Paragraph::new(text).scroll((view_state.scroll_offset as u16, 0));
                    frame.render_widget(widget, body_area);
                }
                if menu_h > 0 {
                    let menu_y = activity_log.y.saturating_add(body_h);
                    let menu_area = Rect::new(activity_log.x, menu_y, activity_log.width, menu_h);
                    render_document_review(frame, state, view_state, menu_area);
                }
            }
            AppMode::MarkdownViewer { content } => {
                let footer_h: u16 = if activity_log.height == 0 {
                    0
                } else if activity_log.height == 1 {
                    1
                } else {
                    2
                };
                let body_h = activity_log.height.saturating_sub(footer_h);
                let body_area =
                    Rect::new(activity_log.x, activity_log.y, activity_log.width, body_h);
                let text = tui_markdown::from_str(content);
                let total_lines = markdown_wrapped_line_count(&text, body_area.width);
                let visible = body_h as usize;
                let max_scroll = if visible == 0 {
                    0
                } else {
                    total_lines.saturating_sub(visible)
                };
                view_state.markdown_scroll_offset =
                    view_state.markdown_scroll_offset.min(max_scroll);
                view_state.markdown_at_end = visible == 0
                    || max_scroll == 0
                    || view_state.markdown_scroll_offset >= max_scroll;
                if body_h > 0 {
                    let widget =
                        Paragraph::new(text).scroll((view_state.markdown_scroll_offset as u16, 0));
                    frame.render_widget(widget, body_area);
                }
                if footer_h > 0 {
                    let footer_area = Rect::new(
                        activity_log.x,
                        activity_log.y.saturating_add(body_h),
                        activity_log.width,
                        footer_h,
                    );
                    log::debug!(
                        "render: plan approval footer ({} lines × {} cols)",
                        footer_h,
                        activity_log.width
                    );
                    if footer_h >= 2 {
                        let approve_prefix = if view_state.markdown_end_button_selected == 0 {
                            "> "
                        } else {
                            "  "
                        };
                        let reject_prefix = if view_state.markdown_end_button_selected == 1 {
                            "> "
                        } else {
                            "  "
                        };
                        let approve_line = format!("{}Approve", approve_prefix);
                        let reject_line = format!("{}Reject", reject_prefix);
                        let approve_style = if view_state.markdown_end_button_selected == 0 {
                            Style::default().add_modifier(Modifier::REVERSED)
                        } else {
                            Style::default()
                        };
                        let reject_style = if view_state.markdown_end_button_selected == 1 {
                            Style::default().add_modifier(Modifier::REVERSED)
                        } else {
                            Style::default()
                        };
                        let lines = vec![
                            Line::from(Span::styled(approve_line, approve_style)),
                            Line::from(Span::styled(reject_line, reject_style)),
                        ];
                        frame.render_widget(Paragraph::new(lines), footer_area);
                    } else {
                        let w = activity_log.width as usize;
                        let half = w / 2;
                        let approve_prefix = if view_state.markdown_end_button_selected == 0 {
                            "> "
                        } else {
                            "  "
                        };
                        let reject_prefix = if view_state.markdown_end_button_selected == 1 {
                            "> "
                        } else {
                            "  "
                        };
                        let left =
                            format!("{:<w1$}", format!("{}Approve", approve_prefix), w1 = half);
                        let right =
                            format!("{:>w2$}", format!("{}Reject", reject_prefix), w2 = w - half);
                        let left_style = if view_state.markdown_end_button_selected == 0 {
                            Style::default().add_modifier(Modifier::REVERSED)
                        } else {
                            Style::default()
                        };
                        let right_style = if view_state.markdown_end_button_selected == 1 {
                            Style::default().add_modifier(Modifier::REVERSED)
                        } else {
                            Style::default()
                        };
                        let footer_line = Line::from(vec![
                            Span::styled(left, left_style),
                            Span::styled(right, right_style),
                        ]);
                        frame.render_widget(Paragraph::new(footer_line), footer_area);
                    }
                }
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
            AppMode::ErrorRecovery { .. } => {
                render_error_recovery(frame, state, view_state, dynamic_area);
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
        let text = status_bar_text_for_draw(state, view_state);
        let style = status_bar_style_for_goal(state.current_goal.as_deref());
        let widget = Paragraph::new(text).style(style);
        frame.render_widget(widget, status_bar);
    }

    if prompt_bar.height > 0 {
        // Split by characters (not words) so the height matches prompt_height's div_ceil
        // calculation. Word-wrapping puts a short prefix like "> " alone on its own line when
        // followed by a long single-word payload, causing the last partial row to overflow the
        // allocated height and be clipped.
        let w = prompt_bar.width as usize;
        let lines: Vec<ratatui::text::Line> = if w == 0 {
            vec![ratatui::text::Line::raw(prompt_text_str.as_str())]
        } else {
            prompt_text_str
                .chars()
                .collect::<Vec<_>>()
                .chunks(w)
                .map(|chunk| ratatui::text::Line::raw(chunk.iter().collect::<String>()))
                .collect()
        };
        let widget = Paragraph::new(lines);
        frame.render_widget(widget, prompt_bar);
    }

    // Advance fast spinner phase only while agent-active (Running). Clarification wait uses a 1 Hz
    // idle dot driven by wall time, not `spinner_tick`.
    if area.width >= 2 && area.height >= 1 && status_activity_is_agent_active(&state.mode) {
        view_state.spinner_tick = view_state.spinner_tick.wrapping_add(1);
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
            initial_selected: _,
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
                let text = if opt.description.is_empty() {
                    format!("{}{}", prefix, opt.label)
                } else {
                    format!("{}{} -- {}", prefix, opt.label, opt.description)
                };
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
                let text = if opt.description.is_empty() {
                    format!("{}{} {}", prefix, checkbox, opt.label)
                } else {
                    format!(
                        "{}{} {} -- {}",
                        prefix, checkbox, opt.label, opt.description
                    )
                };
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
fn render_document_review(
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
        let prefix = if view_state.document_review_selected == i {
            "> "
        } else {
            "  "
        };
        let text = format!("{}{} -- {}", prefix, label, desc);
        let style = if view_state.document_review_selected == i {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    let widget = Paragraph::new(lines);
    frame.render_widget(widget, area);
}

/// Render error recovery: error message + 3 selectable options.
fn render_error_recovery(
    frame: &mut Frame,
    state: &PresenterState,
    view_state: &ViewState,
    area: ratatui::layout::Rect,
) {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};

    if area.height == 0 {
        return;
    }

    let error_msg = match &state.mode {
        AppMode::ErrorRecovery { error_message } => error_message.as_str(),
        _ => return,
    };

    let options = ["Resume", "Continue with agent", "Exit"];
    let highlight = Style::default().add_modifier(Modifier::REVERSED);
    let normal = Style::default();

    let mut lines = vec![
        Line::from(Span::styled(
            format!("Error: {}", error_msg),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    for (i, label) in options.iter().enumerate() {
        let selected = view_state.error_recovery_selected == i;
        let prefix = if selected { "> " } else { "  " };
        let style = if selected { highlight } else { normal };
        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, label),
            style,
        )));
    }

    frame.render_widget(Paragraph::new(lines), area);
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
    use tddy_core::{AppMode, ClarificationQuestion, PresenterState, QuestionOption};

    use crate::mouse_map::LayoutAreas;
    use ratatui::buffer::Buffer;
    use ratatui::layout::{Position, Rect};

    fn status_bar_row_compact_prefix(buffer: &Buffer, area: Rect) -> String {
        let y = area.y;
        (area.x..area.x.saturating_add(area.width))
            .filter_map(|col| {
                buffer
                    .cell(Position::new(col, y))
                    .map(|c| c.symbol().chars().next().unwrap_or(' '))
            })
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    /// Acceptance (PRD): Virtual TUI uses the same `draw()` path as the local TUI; autonomous
    /// periodic re-renders must advance the spinner and elapsed timer without freezing. After
    /// relocation, the cycling spinner frame must be the leading content of the status bar row
    /// (before `Goal:`), not only a detached top-right cell.
    #[test]
    fn virtual_tui_still_emits_bytes_while_idle() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = PresenterState {
            agent: "agent".to_string(),
            model: "model".to_string(),
            mode: AppMode::Running,
            current_goal: Some("plan".to_string()),
            current_state: Some("Running".to_string()),
            workflow_session_id: None,
            goal_start_time: Instant::now(),
            activity_log: Vec::new(),
            inbox: Vec::new(),
            should_quit: false,
            exit_action: None,
            plan_refinement_pending: false,
        };
        let mut vs = ViewState::new();
        let mut areas = LayoutAreas {
            activity_log: Rect::default(),
            dynamic_area: Rect::default(),
            status_bar: Rect::default(),
            prompt_bar: Rect::default(),
        };
        terminal
            .draw(|f| {
                draw(f, &state, &mut vs, false, Some(&mut areas));
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        let line = status_bar_row_compact_prefix(buf, areas.status_bar);
        let first = line.chars().next();
        assert!(
            first.is_some_and(|c| super::SPINNER_FRAMES.contains(&c)),
            "status bar must lead with a spinner frame before Goal: (shared with Virtual TUI); \
             first char={first:?} line={line:?}"
        );
        let goal_pos = line.find("Goal:").expect("Goal: in status bar");
        let spin_pos = line
            .chars()
            .enumerate()
            .find(|(_, c)| super::SPINNER_FRAMES.contains(c))
            .map(|(i, _)| i)
            .expect("spinner in line");
        assert!(
            spin_pos < goal_pos,
            "spinner must appear before Goal: in status line, got {line:?}"
        );
    }

    fn make_state(mode: AppMode) -> PresenterState {
        PresenterState {
            agent: "test-agent".to_string(),
            model: "test-model".to_string(),
            mode,
            current_goal: None,
            current_state: None,
            workflow_session_id: None,
            goal_start_time: Instant::now(),
            activity_log: Vec::new(),
            inbox: Vec::new(),
            should_quit: false,
            exit_action: None,
            plan_refinement_pending: false,
        }
    }

    /// Third pipe-separated segment in the `… │ … │ …` status tail (elapsed like `3s`, or `Ready` in idle).
    fn elapsed_segment_from_goal_status_line(line: &str) -> Option<String> {
        let parts: Vec<&str> = line.split(" │ ").collect();
        if parts.len() >= 3 {
            Some(parts[2].to_string())
        } else {
            None
        }
    }

    fn first_activity_char(line: &str) -> Option<char> {
        line.chars().next()
    }

    /// PRD: idle clarification wait uses middle-dot / bullet pulse (not `SPINNER_FRAMES`).
    const IDLE_DOT_PULSE_CHARS: &[char] = &['·', '•'];

    fn select_mode_with_goal() -> AppMode {
        AppMode::Select {
            question: ClarificationQuestion {
                header: "Scope".to_string(),
                question: "Pick one".to_string(),
                options: vec![QuestionOption {
                    label: "A".to_string(),
                    description: String::new(),
                }],
                multi_select: false,
                allow_other: false,
            },
            question_index: 0,
            total_questions: 1,
            initial_selected: 0,
        }
    }

    /// Acceptance (PRD): `status_bar_frozen_elapsed_in_select_mode` — in Select with an active goal,
    /// the compact elapsed token in the status line must not advance across wall time while the
    /// mode is unchanged (clock frozen during clarification wait).
    #[test]
    fn status_bar_frozen_elapsed_in_select_mode() {
        let mut state = make_state(select_mode_with_goal());
        state.current_goal = Some("plan".to_string());
        state.current_state = Some("Running".to_string());
        state.goal_start_time = Instant::now();
        let mut vs = ViewState::new();

        let before = status_bar_text_for_draw(&state, &mut vs);
        let e0 = elapsed_segment_from_goal_status_line(&before).expect("elapsed segment");
        std::thread::sleep(std::time::Duration::from_millis(1100));
        vs.spinner_tick = vs.spinner_tick.wrapping_add(400);
        let after = status_bar_text_for_draw(&state, &mut vs);
        let e1 = elapsed_segment_from_goal_status_line(&after).expect("elapsed segment");

        assert_eq!(
            e0, e1,
            "PRD: elapsed display must stay fixed in Select while waiting; before={before:?} after={after:?}"
        );
    }

    /// Acceptance (PRD): `status_bar_idle_dot_not_spinner_in_text_input` — TextInput wait must use
    /// the idle dot pulse (`·` / `•`), not characters from `SPINNER_FRAMES`.
    #[test]
    fn status_bar_idle_dot_not_spinner_in_text_input() {
        let mut state = make_state(AppMode::TextInput {
            prompt: "Why?".to_string(),
        });
        state.current_goal = Some("acceptance-tests".to_string());
        state.current_state = Some("Running".to_string());
        state.goal_start_time = Instant::now();
        let mut vs = ViewState::new();

        let line = status_bar_text_for_draw(&state, &mut vs);
        let lead = first_activity_char(&line).expect("leading char");
        assert!(
            !super::SPINNER_FRAMES.contains(&lead),
            "expected idle dot prefix, not spinner frame {lead:?} in {line:?}"
        );
        assert!(
            IDLE_DOT_PULSE_CHARS.contains(&lead),
            "expected idle pulse glyph (· or •), got {lead:?} in {line:?}"
        );
    }

    /// Acceptance (PRD): `status_bar_running_mode_uses_spinner_and_live_elapsed` — Running keeps a
    /// fast spinner and a live elapsed clock; user-wait modes must not cycle `SPINNER_FRAMES` on
    /// tick advances (idle dot only).
    #[test]
    fn status_bar_running_mode_uses_spinner_and_live_elapsed() {
        let start = Instant::now();
        let mut running = make_state(AppMode::Running);
        running.current_goal = Some("plan".to_string());
        running.current_state = Some("Running".to_string());
        running.goal_start_time = start;
        let mut vs_run = ViewState::new();
        let line_tick0 = status_bar_text_for_draw(&running, &mut vs_run);
        vs_run.spinner_tick = 4;
        let line_tick4 = status_bar_text_for_draw(&running, &mut vs_run);
        let c0 = first_activity_char(&line_tick0).unwrap();
        let c4 = first_activity_char(&line_tick4).unwrap();
        assert!(
            super::SPINNER_FRAMES.contains(&c0) && super::SPINNER_FRAMES.contains(&c4),
            "Running mode should use spinner frames; tick0={c0:?} tick4={c4:?}"
        );
        assert_ne!(
            c0, c4,
            "Running spinner should advance between tick 0 and 4; lines {line_tick0:?} {line_tick4:?}"
        );

        std::thread::sleep(std::time::Duration::from_millis(1100));
        let line_late = status_bar_text_for_draw(&running, &mut vs_run);
        let e_early = elapsed_segment_from_goal_status_line(&line_tick0).unwrap();
        let e_late = elapsed_segment_from_goal_status_line(&line_late).unwrap();
        assert_ne!(
            e_early, e_late,
            "Running elapsed display should advance over wall time; early={e_early} late={e_late}"
        );

        let mut state_sel = make_state(select_mode_with_goal());
        state_sel.current_goal = Some("plan".to_string());
        state_sel.current_state = Some("Running".to_string());
        state_sel.goal_start_time = start;
        let mut vs_sel = ViewState::new();
        let s0 = first_activity_char(&status_bar_text_for_draw(&state_sel, &mut vs_sel)).unwrap();
        vs_sel.spinner_tick = 4;
        let s4 = first_activity_char(&status_bar_text_for_draw(&state_sel, &mut vs_sel)).unwrap();
        assert!(
            IDLE_DOT_PULSE_CHARS.contains(&s0) && IDLE_DOT_PULSE_CHARS.contains(&s4),
            "Select wait must use idle dot glyphs only, not spinner; s0={s0:?} s4={s4:?}"
        );
        assert_eq!(
            s0, s4,
            "Select idle prefix should not advance on fast ticks (1 Hz pulse); s0={s0:?} s4={s4:?}"
        );
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

    #[test]
    fn test_render_error_recovery_shows_three_options() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(60, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let state = make_state(AppMode::ErrorRecovery {
            error_message: "backend timeout".to_string(),
        });
        let vs = ViewState::new(); // error_recovery_selected defaults to 0

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_error_recovery(frame, &state, &vs, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        let content: String = buffer
            .content
            .iter()
            .map(|cell| cell.symbol().chars().next().unwrap_or(' '))
            .collect();

        assert!(
            content.contains("Resume"),
            "Error recovery should render 'Resume' option, got: {}",
            content.trim()
        );
        assert!(
            content.contains("Continue with agent"),
            "Error recovery should render 'Continue with agent' option, got: {}",
            content.trim()
        );
        assert!(
            content.contains("Exit"),
            "Error recovery should render 'Exit' option, got: {}",
            content.trim()
        );
        assert!(
            content.contains("backend timeout"),
            "Error recovery should render the error message, got: {}",
            content.trim()
        );
    }

    /// PRD: While the PRD is visible in the activity pane, the prompt bar tells the user they can
    /// type refinement feedback (e.g. after Reject or when the refine affordance is focused).
    #[test]
    fn markdown_viewer_prompt_shows_refinement_hint_when_reject_or_focused() {
        let state = make_state(AppMode::MarkdownViewer {
            content: "# My PRD".to_string(),
        });
        let mut vs = ViewState::new();
        vs.markdown_at_end = true;
        vs.markdown_end_button_selected = 1;
        let text = prompt_text(&state, &vs);
        let lower = text.to_lowercase();
        assert!(
            lower.contains("feedback") && text.contains("Enter"),
            "prompt must describe typing refinement feedback and submission while PRD remains visible; got {:?}",
            text
        );
    }

    #[test]
    fn plan_review_frame_shows_view_approve_refine_menu() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = make_state(AppMode::DocumentReview {
            content: "# Plan".to_string(),
        });
        let mut vs = ViewState::new();
        vs.document_review_selected = 1;
        let mut areas = LayoutAreas {
            activity_log: Rect::default(),
            dynamic_area: Rect::default(),
            status_bar: Rect::default(),
            prompt_bar: Rect::default(),
        };
        terminal
            .draw(|f| {
                draw(f, &state, &mut vs, false, Some(&mut areas));
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer
            .content
            .iter()
            .map(|cell| cell.symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(
            content.contains("Plan generated. Choose an action:"),
            "expected plan approval menu header in rendered frame"
        );
        assert!(
            content.contains("View") && content.contains("Approve") && content.contains("Refine"),
            "expected View, Approve, and Refine labels from the plan approval menu in the frame"
        );
    }

    #[test]
    fn markdown_viewer_draw_marks_at_end_when_scrolled_to_bottom() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = make_state(AppMode::MarkdownViewer {
            content: "# Long PRD\n\n".to_string() + &"line\n".repeat(200),
        });
        let mut vs = ViewState::new();
        vs.markdown_scroll_offset = 50_000;
        let mut areas = LayoutAreas {
            activity_log: Rect::default(),
            dynamic_area: Rect::default(),
            status_bar: Rect::default(),
            prompt_bar: Rect::default(),
        };
        terminal
            .draw(|f| {
                draw(f, &state, &mut vs, false, Some(&mut areas));
            })
            .unwrap();
        assert!(
            vs.markdown_at_end,
            "scrolling to the document end must enable footer Approve/Reject navigation"
        );
    }

    fn count_reversed_on_footer_line(
        buffer: &Buffer,
        activity: Rect,
        line_index_in_footer: u16,
    ) -> usize {
        let footer_h = if activity.height >= 2 { 2 } else { 1 }.min(activity.height);
        let footer_top = activity.y + activity.height - footer_h;
        let y = footer_top + line_index_in_footer;
        (activity.x..activity.x + activity.width)
            .filter(|&x| {
                buffer
                    .cell(Position::new(x, y))
                    .is_some_and(|c| c.style().add_modifier.contains(Modifier::REVERSED))
            })
            .count()
    }

    #[test]
    fn markdown_viewer_footer_reverses_approve_line_when_approve_selected() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = make_state(AppMode::MarkdownViewer {
            content: "# p".to_string(),
        });
        let mut vs = ViewState::new();
        vs.markdown_at_end = true;
        vs.markdown_end_button_selected = 0;
        let mut areas = LayoutAreas {
            activity_log: Rect::default(),
            dynamic_area: Rect::default(),
            status_bar: Rect::default(),
            prompt_bar: Rect::default(),
        };
        terminal
            .draw(|f| {
                draw(f, &state, &mut vs, false, Some(&mut areas));
            })
            .unwrap();
        assert!(
            areas.activity_log.height >= 2,
            "fixture expects a two-line plan footer (stacked Approve / Reject)"
        );
        let buffer = terminal.backend().buffer();
        let approve_line = count_reversed_on_footer_line(buffer, areas.activity_log, 0);
        let reject_line = count_reversed_on_footer_line(buffer, areas.activity_log, 1);
        assert!(
            approve_line > 0 && reject_line == 0,
            "only the Approve line must use REVERSED when Approve is focused; approve_line={approve_line} reject_line={reject_line}"
        );
    }

    #[test]
    fn markdown_viewer_footer_reverses_reject_line_when_reject_selected() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = make_state(AppMode::MarkdownViewer {
            content: "# p".to_string(),
        });
        let mut vs = ViewState::new();
        vs.markdown_at_end = true;
        vs.markdown_end_button_selected = 1;
        let mut areas = LayoutAreas {
            activity_log: Rect::default(),
            dynamic_area: Rect::default(),
            status_bar: Rect::default(),
            prompt_bar: Rect::default(),
        };
        terminal
            .draw(|f| {
                draw(f, &state, &mut vs, false, Some(&mut areas));
            })
            .unwrap();
        assert!(
            areas.activity_log.height >= 2,
            "fixture expects a two-line plan footer (stacked Approve / Reject)"
        );
        let buffer = terminal.backend().buffer();
        let approve_line = count_reversed_on_footer_line(buffer, areas.activity_log, 0);
        let reject_line = count_reversed_on_footer_line(buffer, areas.activity_log, 1);
        assert!(
            approve_line == 0 && reject_line > 0,
            "only the Reject line must use REVERSED when Reject is focused; approve_line={approve_line} reject_line={reject_line}"
        );
    }

    #[test]
    fn markdown_viewer_footer_shows_approve_and_reject_options() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = make_state(AppMode::MarkdownViewer {
            content: "# p".to_string(),
        });
        let mut vs = ViewState::new();
        vs.markdown_at_end = true;
        let mut areas = LayoutAreas {
            activity_log: Rect::default(),
            dynamic_area: Rect::default(),
            status_bar: Rect::default(),
            prompt_bar: Rect::default(),
        };
        terminal
            .draw(|f| {
                draw(f, &state, &mut vs, false, Some(&mut areas));
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let content: String = buffer
            .content
            .iter()
            .map(|cell| cell.symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(
            content.contains("Approve") && content.contains("Reject"),
            "activity footer must label the two actions Approve and Reject when the plan end is reached"
        );
    }

    #[test]
    fn markdown_viewer_reject_button_visible_in_short_terminal() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 4);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = make_state(AppMode::MarkdownViewer {
            content: "# Plan".to_string(),
        });
        let mut vs = ViewState::new();
        let mut areas = LayoutAreas {
            activity_log: Rect::default(),
            dynamic_area: Rect::default(),
            status_bar: Rect::default(),
            prompt_bar: Rect::default(),
        };
        terminal
            .draw(|f| {
                draw(f, &state, &mut vs, false, Some(&mut areas));
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let flat: String = buffer
            .content
            .iter()
            .map(|cell| cell.symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(
            flat.contains("Reject"),
            "Reject must stay visible in the plan viewer footer"
        );
    }
}
