//! Frame rendering: draw activity log, status bar, prompt bar.

use ratatui::widgets::Paragraph;
use ratatui::Frame;

use tddy_core::{ActivityEntry, AppMode, PresenterState};

use crate::layout::{debug_log_height, inbox_height, layout_chunks_with_inbox};
use crate::ui::{format_status_bar, status_bar_style_for_goal};
use crate::view_state::{InboxFocus, ViewState};

/// Draw the TUI layout: activity log, status bar, prompt bar.
/// When `debug` is true, the debug log area is shown; otherwise it is hidden.
pub fn draw(frame: &mut Frame, state: &PresenterState, view_state: &ViewState, debug: bool) {
    let is_running = matches!(state.mode, AppMode::Running);
    let inbox_h = inbox_height(state.inbox.len(), is_running);
    let debug_logs = if debug {
        tddy_core::get_buffered_logs()
    } else {
        vec![]
    };
    let debug_h = debug_log_height(debug_logs.len());

    let (activity_log, _status_spacer, inbox_area, status_bar, debug_log, prompt_bar) =
        layout_chunks_with_inbox(frame.area(), inbox_h, debug_h);

    if activity_log.height > 0 {
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

    if inbox_area.height > 0 {
        render_inbox(frame, state, view_state, inbox_area);
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
        let text = match &state.mode {
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
            AppMode::DemoPrompt => "> Run demo? [r] Run  [s] Skip".to_string(),
            AppMode::Done => "> Workflow complete. Press Enter to exit.".to_string(),
            _ => String::new(),
        };
        let widget = Paragraph::new(text);
        frame.render_widget(widget, prompt_bar);
    }
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
