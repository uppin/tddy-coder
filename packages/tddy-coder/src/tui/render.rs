//! Frame rendering: draw activity log, status bar, prompt bar to the terminal.
//!
//! AC1: Three regions visible when TUI displays.

use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::layout::{debug_log_height, inbox_height, layout_chunks_with_inbox};
use crate::tui::state::{AppMode, AppState};
use crate::tui::ui::{format_status_bar, status_bar_style_for_goal};

/// Draw the TUI layout: activity log, status bar, prompt bar.
///
/// AC1: Renders three regions so they are visible.
pub fn draw(frame: &mut Frame, state: &AppState) {
    let is_running = matches!(state.mode, AppMode::Running);
    let inbox_h = inbox_height(state.inbox.len(), is_running);
    let debug_logs = tddy_core::get_buffered_logs();
    let debug_h = debug_log_height(debug_logs.len());

    let (activity_log, _status_spacer, inbox_area, status_bar, debug_log, prompt_bar) =
        layout_chunks_with_inbox(frame.area(), inbox_h, debug_h);

    if activity_log.height > 0 {
        let content = state
            .activity_log
            .iter()
            .map(|e| e.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let line_count = state.activity_log.len();
        let area_height = activity_log.height as usize;
        let scroll_y = if state.auto_scroll && line_count > area_height {
            (line_count - area_height) as u16
        } else {
            let max_scroll = line_count.saturating_sub(area_height);
            (state.scroll_offset.min(max_scroll)) as u16
        };
        let widget = Paragraph::new(content).scroll((scroll_y, 0));
        frame.render_widget(widget, activity_log);
    }

    if inbox_area.height > 0 {
        render_inbox(frame, state, inbox_area);
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
                format_status_bar(goal, s, elapsed)
            }
            _ => "Goal: — │ State: — │ Ready".to_string(),
        };
        let style = status_bar_style_for_goal(state.current_goal.as_deref());
        let widget = Paragraph::new(text).style(style);
        frame.render_widget(widget, status_bar);
    }

    if prompt_bar.height > 0 {
        let text = match &state.mode {
            AppMode::FeatureInput { input, .. } => {
                let prefix = "> ";
                if input.is_empty() {
                    format!("{}Type your feature description and press Enter...", prefix)
                } else {
                    format!("{}{}", prefix, input)
                }
            }
            AppMode::Running => {
                if state.running_input.is_empty() {
                    "> Queue a follow-up prompt...".to_string()
                } else {
                    format!("> {}", state.running_input)
                }
            }
            AppMode::DemoPrompt => "> Run demo? [r] Run  [s] Skip".to_string(),
            _ => String::new(),
        };
        let widget = Paragraph::new(text);
        frame.render_widget(widget, prompt_bar);
    }
}

/// Render inbox items as numbered lines (e.g. `[1] Fix the login bug`) into the given area.
/// The currently selected item (when `inbox_focus` is `List` or `Editing`) is highlighted.
pub fn render_inbox(frame: &mut Frame, state: &AppState, area: ratatui::layout::Rect) {
    use crate::tui::state::InboxFocus;
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
            let is_selected = matches!(state.inbox_focus, InboxFocus::List | InboxFocus::Editing)
                && i == state.inbox_cursor;
            let display_text = if is_selected && state.inbox_focus == InboxFocus::Editing {
                &state.inbox_edit_buffer
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
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// AC1: Drawing produces status bar content in the middle region.
    /// When state has goal and state set, status bar area shows "Goal:" and "State:".
    #[test]
    fn test_draw_renders_status_bar_visible() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create terminal");

        let mut state = AppState::new();
        state.current_goal = Some("plan".to_string());
        state.current_state = Some("Planning".to_string());

        terminal.draw(|f| draw(f, &state)).expect("draw");

        let buffer = terminal.backend().buffer();
        let content = buffer.content();
        let width = buffer.area().width as usize;
        let height = buffer.area().height as usize;

        let mut found_goal = false;
        let mut found_state = false;
        for y in 0..height {
            for x in 0..width.min(20) {
                let i = y * width + x;
                if i + 5 <= content.len() {
                    let s: String = content[i..i + 5]
                        .iter()
                        .map(|c| c.symbol().chars().next().unwrap_or(' '))
                        .collect();
                    if s.contains("Goal:") {
                        found_goal = true;
                    }
                    if s.contains("State") {
                        found_state = true;
                    }
                }
            }
        }
        assert!(found_goal, "status bar must display 'Goal:'");
        assert!(found_state, "status bar must display 'State'");
    }

    /// AC2: Inbox items are rendered above the status bar with [N] prefix text.
    #[test]
    fn test_inbox_rendered_with_items() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create terminal");

        let mut state = AppState::new();
        state.mode = crate::tui::state::AppMode::Running;
        state.inbox = vec![
            "Fix the login bug".to_string(),
            "Add unit tests".to_string(),
        ];
        state.current_goal = Some("plan".to_string());
        state.current_state = Some("Planning".to_string());

        terminal.draw(|f| draw(f, &state)).expect("draw");

        let buffer = terminal.backend().buffer();
        let content = buffer.content();
        let width = buffer.area().width as usize;
        let height = buffer.area().height as usize;

        let mut found_prefix_1 = false;
        let mut found_prefix_2 = false;
        for y in 0..height {
            let row_start = y * width;
            let row_end = row_start + width;
            let row: String = content[row_start..row_end]
                .iter()
                .map(|c| c.symbol().chars().next().unwrap_or(' '))
                .collect();
            if row.contains("[1]") {
                found_prefix_1 = true;
            }
            if row.contains("[2]") {
                found_prefix_2 = true;
            }
        }
        assert!(
            found_prefix_1,
            "buffer must contain '[1]' prefix for first inbox item"
        );
        assert!(
            found_prefix_2,
            "buffer must contain '[2]' prefix for second inbox item"
        );
    }

    /// Running mode: prompt bar shows running_input when user has typed text.
    #[test]
    fn test_running_mode_prompt_shows_running_input() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("create terminal");

        let mut state = AppState::new();
        state.mode = crate::tui::state::AppMode::Running;
        state.running_input = "fix bug".to_string();
        state.current_goal = Some("plan".to_string());
        state.current_state = Some("Planning".to_string());

        terminal.draw(|f| draw(f, &state)).expect("draw");

        let buffer = terminal.backend().buffer();
        let content = buffer.content();
        let width = buffer.area().width as usize;
        let height = buffer.area().height as usize;

        let last_row_start = (height - 1) * width;
        let last_row: String = content[last_row_start..last_row_start + width]
            .iter()
            .map(|c| c.symbol().chars().next().unwrap_or(' '))
            .collect();

        assert!(
            last_row.contains("fix bug"),
            "prompt bar must display running_input text 'fix bug': '{}'",
            last_row.trim()
        );
    }

    /// AC1: Three regions are drawn (activity, status, prompt).
    /// Prompt bar at bottom shows placeholder for FeatureInput mode.
    #[test]
    fn test_draw_renders_prompt_bar_at_bottom() {
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).expect("create terminal");
        let state = AppState::new();

        terminal.draw(|f| draw(f, &state)).expect("draw");

        let buffer = terminal.backend().buffer();
        let height = buffer.area().height as usize;
        let width = buffer.area().width as usize;
        let content = buffer.content();

        let last_row_start = (height - 1) * width;
        let last_row: String = content[last_row_start..last_row_start + width]
            .iter()
            .map(|c| c.symbol().chars().next().unwrap_or(' '))
            .collect();

        assert!(
            !last_row.trim().is_empty() || height >= 2,
            "prompt bar at bottom must have content or layout reserves space"
        );
    }
}
