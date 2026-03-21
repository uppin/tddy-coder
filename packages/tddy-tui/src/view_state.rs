//! View-local state: scroll, text buffers, selection cursor.
//!
//! The Presenter owns application state; the View owns this view-local state
//! (editing buffers, cursor positions, scroll offset).

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tddy_core::AppMode;
use tddy_core::ClarificationQuestion;

/// Which sub-element has focus when the inbox is visible during Running mode.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum InboxFocus {
    /// Focus is on the running-mode input field (default).
    #[default]
    None,
    /// Focus is on the inbox list; user can navigate with Up/Down.
    List,
    /// User is editing the selected inbox item in-place.
    Editing,
}

/// View-local state: buffers, cursors, scroll. Not owned by the Presenter.
#[derive(Debug, Default)]
pub struct ViewState {
    /// Manual scroll offset (lines from top). Used when auto_scroll is false.
    pub scroll_offset: usize,
    /// Feature input buffer (FeatureInput mode).
    pub feature_input: String,
    /// Cursor position in feature_input.
    pub feature_cursor: usize,
    /// Text the user is typing in the prompt bar during Running mode.
    pub running_input: String,
    /// Cursor position within running_input.
    pub running_cursor: usize,
    /// Currently highlighted option index for Select mode (0-based, includes "Other" as last).
    pub select_selected: usize,
    /// Text typed when "Other (type your own)" is selected in Select mode.
    pub select_other_text: String,
    /// True when the user has selected "Other" and is typing a custom answer.
    pub select_typing_other: bool,
    /// Cursor for MultiSelect mode.
    pub multiselect_cursor: usize,
    /// Checked state per option (indices match original options + 1 for "Other").
    pub multiselect_checked: Vec<bool>,
    /// Text typed for the "Other" option in MultiSelect.
    pub multiselect_other_text: String,
    /// True when typing custom answer for "Other" in MultiSelect.
    pub multiselect_typing_other: bool,
    /// Text input buffer for TextInput mode.
    pub text_input: String,
    /// Cursor in text_input.
    pub text_input_cursor: usize,
    /// Currently selected item index in the inbox list.
    pub inbox_cursor: usize,
    /// Which sub-element has focus when inbox is visible.
    pub inbox_focus: InboxFocus,
    /// Buffer for editing an inbox item in-place.
    pub inbox_edit_buffer: String,
    /// Selected option in PlanReview mode (0=View, 1=Approve, 2=Refine).
    pub plan_review_selected: usize,
    /// Scroll offset for MarkdownViewer mode.
    pub markdown_scroll_offset: usize,
    /// True when the MarkdownViewer has scrolled to the end of the document.
    pub markdown_at_end: bool,
    /// Index of the selected inline button at end of document (0=Approve, 1=Refine).
    pub markdown_end_button_selected: usize,
    /// Selected option index in ErrorRecovery mode (0=Resume, 1=Exit).
    pub error_recovery_selected: usize,
    /// Frame counter for the spinner animation.
    pub spinner_tick: usize,
}

/// Byte index of the start of the character immediately before `idx`.
/// Cursors must be byte indices for `String::insert`/`remove` to work with multi-byte UTF-8.
fn prev_char_boundary(s: &str, idx: usize) -> usize {
    if idx <= 1 {
        return 0;
    }
    let mut i = idx - 1;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Advance byte index by one character. Returns `idx` if at end.
fn advance_cursor_by_char(s: &str, idx: usize) -> usize {
    if idx >= s.len() {
        return idx;
    }
    s[idx..]
        .chars()
        .next()
        .map(|c| idx + c.len_utf8())
        .unwrap_or(idx)
}

impl ViewState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset view state when entering a new mode. Call from TuiView when mode changes.
    pub fn on_mode_changed(&mut self, mode: &AppMode) {
        match mode {
            AppMode::FeatureInput => {
                self.feature_input.clear();
                self.feature_cursor = 0;
            }
            AppMode::Select {
                initial_selected, ..
            } => {
                self.select_selected = *initial_selected;
                self.select_other_text.clear();
                self.select_typing_other = false;
            }
            AppMode::MultiSelect { question, .. } => {
                let len = question.options.len() + if question.allow_other { 1 } else { 0 };
                self.multiselect_cursor = 0;
                self.multiselect_checked = vec![false; len];
                self.multiselect_other_text.clear();
                self.multiselect_typing_other = false;
            }
            AppMode::TextInput { .. } => {
                self.text_input.clear();
                self.text_input_cursor = 0;
            }
            AppMode::Running => {
                self.running_input.clear();
                self.running_cursor = 0;
                self.inbox_focus = InboxFocus::None;
            }
            AppMode::PlanReview { .. } => {
                self.plan_review_selected = 0;
            }
            AppMode::MarkdownViewer { .. } => {
                self.markdown_scroll_offset = 0;
                self.markdown_at_end = false;
                self.markdown_end_button_selected = 0;
            }
            AppMode::Done => {}
            AppMode::ErrorRecovery { .. } => {
                log::debug!("on_mode_changed: ErrorRecovery — resetting error_recovery_selected");
                self.error_recovery_selected = 0;
            }
        }
    }

    /// Handle a key event that updates view-local state only (no UserIntent).
    /// Call before key_event_to_intent. Returns true if the key was consumed.
    /// `inbox_len`: when in Running mode with inbox, pass Some(len) for cursor clamping.
    pub fn handle_key_view_local(
        &mut self,
        key: KeyEvent,
        mode: &AppMode,
        inbox_len: usize,
    ) -> bool {
        if key.kind != KeyEventKind::Press {
            return false;
        }

        match mode {
            AppMode::FeatureInput => self.handle_feature_input_key(key),
            AppMode::Running => self.handle_running_key_view_local(key, inbox_len),
            AppMode::PlanReview { .. } => self.handle_plan_review_key_view_local(key),
            AppMode::MarkdownViewer { .. } => self.handle_markdown_viewer_key_view_local(key),
            AppMode::Select { question, .. } => self.handle_select_key_view_local(key, question),
            AppMode::MultiSelect { question, .. } => {
                self.handle_multiselect_key_view_local(key, question)
            }
            AppMode::TextInput { .. } => self.handle_text_input_key_view_local(key),
            AppMode::Done => self.handle_done_key_view_local(key),
            AppMode::ErrorRecovery { .. } => self.handle_error_recovery_key_view_local(key),
        }
    }

    fn handle_error_recovery_key_view_local(&mut self, key: KeyEvent) -> bool {
        const OPTIONS: usize = 3;
        match key.code {
            KeyCode::Up => {
                self.error_recovery_selected = if self.error_recovery_selected == 0 {
                    OPTIONS - 1
                } else {
                    self.error_recovery_selected - 1
                };
                true
            }
            KeyCode::Down => {
                self.error_recovery_selected = if self.error_recovery_selected >= OPTIONS - 1 {
                    0
                } else {
                    self.error_recovery_selected + 1
                };
                true
            }
            _ => false,
        }
    }

    fn handle_done_key_view_local(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(5);
                true
            }
            KeyCode::PageDown => {
                self.scroll_offset = self.scroll_offset.saturating_add(5);
                true
            }
            _ => false,
        }
    }

    fn handle_plan_review_key_view_local(&mut self, key: KeyEvent) -> bool {
        const OPTIONS: usize = 3; // View, Approve, Refine
        match key.code {
            KeyCode::Up => {
                self.plan_review_selected = if self.plan_review_selected == 0 {
                    OPTIONS - 1
                } else {
                    self.plan_review_selected - 1
                };
                true
            }
            KeyCode::Down => {
                self.plan_review_selected = if self.plan_review_selected >= OPTIONS - 1 {
                    0
                } else {
                    self.plan_review_selected + 1
                };
                true
            }
            _ => false,
        }
    }

    fn handle_markdown_viewer_key_view_local(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char(' ') => {
                self.markdown_scroll_offset = self.markdown_scroll_offset.saturating_add(10);
                true
            }
            KeyCode::PageUp => {
                self.markdown_scroll_offset = self.markdown_scroll_offset.saturating_sub(10);
                true
            }
            KeyCode::PageDown => {
                self.markdown_scroll_offset = self.markdown_scroll_offset.saturating_add(10);
                true
            }
            KeyCode::Up if self.markdown_at_end => {
                self.markdown_end_button_selected =
                    self.markdown_end_button_selected.saturating_sub(1);
                true
            }
            KeyCode::Down if self.markdown_at_end => {
                self.markdown_end_button_selected = (self.markdown_end_button_selected + 1).min(1);
                true
            }
            KeyCode::Up => {
                self.markdown_scroll_offset = self.markdown_scroll_offset.saturating_sub(1);
                true
            }
            KeyCode::Down => {
                self.markdown_scroll_offset = self.markdown_scroll_offset.saturating_add(1);
                true
            }
            _ => false,
        }
    }

    fn handle_feature_input_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            // Ctrl+letter must not be inserted as text (e.g. Ctrl+C = Quit). Real TUI skips
            // handle_key_view_local for Ctrl+C; VirtualTui relies on this guard + key_map.
            KeyCode::Char(c)
                if !c.is_control() && !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.feature_input.insert(self.feature_cursor, c);
                self.feature_cursor += c.len_utf8();
                true
            }
            KeyCode::Backspace if self.feature_cursor > 0 => {
                let prev = prev_char_boundary(&self.feature_input, self.feature_cursor);
                self.feature_cursor = prev;
                self.feature_input.remove(self.feature_cursor);
                true
            }
            KeyCode::Left => {
                self.feature_cursor = prev_char_boundary(&self.feature_input, self.feature_cursor);
                true
            }
            KeyCode::Right => {
                self.feature_cursor =
                    advance_cursor_by_char(&self.feature_input, self.feature_cursor);
                true
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(5);
                true
            }
            KeyCode::PageDown => {
                self.scroll_offset = self.scroll_offset.saturating_add(5);
                true
            }
            _ => false,
        }
    }

    fn handle_running_key_view_local(&mut self, key: KeyEvent, inbox_len: usize) -> bool {
        match self.inbox_focus {
            InboxFocus::None => match key.code {
                KeyCode::Char(c)
                    if !c.is_control() && !key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    self.running_input.insert(self.running_cursor, c);
                    self.running_cursor += c.len_utf8();
                    true
                }
                KeyCode::Backspace if self.running_cursor > 0 => {
                    let prev = prev_char_boundary(&self.running_input, self.running_cursor);
                    self.running_cursor = prev;
                    self.running_input.remove(self.running_cursor);
                    true
                }
                KeyCode::Left => {
                    self.running_cursor =
                        prev_char_boundary(&self.running_input, self.running_cursor);
                    true
                }
                KeyCode::Right => {
                    self.running_cursor =
                        advance_cursor_by_char(&self.running_input, self.running_cursor);
                    true
                }
                KeyCode::PageUp => {
                    self.scroll_offset = self.scroll_offset.saturating_sub(5);
                    true
                }
                KeyCode::PageDown => {
                    self.scroll_offset = self.scroll_offset.saturating_add(5);
                    true
                }
                _ => false,
            },
            InboxFocus::List => match key.code {
                KeyCode::Up => {
                    self.inbox_cursor = self.inbox_cursor.saturating_sub(1);
                    true
                }
                KeyCode::Down => {
                    let max = inbox_len.saturating_sub(1);
                    if self.inbox_cursor < max {
                        self.inbox_cursor += 1;
                    }
                    true
                }
                KeyCode::Char('E') if inbox_len > 0 => {
                    self.inbox_focus = InboxFocus::Editing;
                    // inbox_edit_buffer will be populated by caller from inbox[inbox_cursor]
                    true
                }
                KeyCode::Esc => {
                    self.inbox_focus = InboxFocus::None;
                    true
                }
                KeyCode::PageUp => {
                    self.scroll_offset = self.scroll_offset.saturating_sub(5);
                    true
                }
                KeyCode::PageDown => {
                    self.scroll_offset = self.scroll_offset.saturating_add(5);
                    true
                }
                _ => false,
            },
            InboxFocus::Editing => match key.code {
                KeyCode::Char(c)
                    if !c.is_control() && !key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    self.inbox_edit_buffer.push(c);
                    true
                }
                KeyCode::Backspace => {
                    self.inbox_edit_buffer.pop();
                    true
                }
                KeyCode::Esc => {
                    self.inbox_edit_buffer.clear();
                    self.inbox_focus = InboxFocus::List;
                    true
                }
                KeyCode::PageUp => {
                    self.scroll_offset = self.scroll_offset.saturating_sub(5);
                    true
                }
                KeyCode::PageDown => {
                    self.scroll_offset = self.scroll_offset.saturating_add(5);
                    true
                }
                _ => false,
            },
        }
    }

    fn handle_select_key_view_local(
        &mut self,
        key: KeyEvent,
        question: &ClarificationQuestion,
    ) -> bool {
        let option_count = question.options.len();
        let other_idx = option_count;
        let max_idx = if question.allow_other {
            other_idx
        } else {
            option_count.saturating_sub(1)
        };

        match key.code {
            KeyCode::Up => {
                if self.select_typing_other {
                    false
                } else {
                    self.select_selected = if self.select_selected == 0 {
                        max_idx
                    } else {
                        self.select_selected - 1
                    };
                    true
                }
            }
            KeyCode::Down => {
                if self.select_typing_other {
                    false
                } else {
                    self.select_selected = if self.select_selected >= max_idx {
                        0
                    } else {
                        self.select_selected + 1
                    };
                    true
                }
            }
            KeyCode::Char(c)
                if self.select_typing_other && !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.select_other_text.push(c);
                true
            }
            KeyCode::Char(c)
                if question.allow_other
                    && !c.is_control()
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                    && self.select_selected == other_idx
                    && !self.select_typing_other =>
            {
                self.select_typing_other = true;
                self.select_other_text.push(c);
                true
            }
            KeyCode::Backspace if self.select_typing_other => {
                self.select_other_text.pop();
                true
            }
            KeyCode::Enter
                if question.allow_other
                    && self.select_selected == other_idx
                    && !self.select_typing_other =>
            {
                self.select_typing_other = true;
                true
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(5);
                true
            }
            KeyCode::PageDown => {
                self.scroll_offset = self.scroll_offset.saturating_add(5);
                true
            }
            _ => false,
        }
    }

    fn handle_multiselect_key_view_local(
        &mut self,
        key: KeyEvent,
        question: &ClarificationQuestion,
    ) -> bool {
        let other_idx = question.options.len();
        let max_idx = if question.allow_other {
            other_idx
        } else {
            question.options.len().saturating_sub(1)
        };

        match key.code {
            KeyCode::Up => {
                if !self.multiselect_typing_other {
                    self.multiselect_cursor = if self.multiselect_cursor == 0 {
                        max_idx
                    } else {
                        self.multiselect_cursor - 1
                    };
                    true
                } else {
                    false
                }
            }
            KeyCode::Down => {
                if !self.multiselect_typing_other {
                    self.multiselect_cursor = if self.multiselect_cursor >= max_idx {
                        0
                    } else {
                        self.multiselect_cursor + 1
                    };
                    true
                } else {
                    false
                }
            }
            KeyCode::Char(' ') if !self.multiselect_typing_other => {
                if self.multiselect_cursor < other_idx {
                    if let Some(c) = self.multiselect_checked.get_mut(self.multiselect_cursor) {
                        *c = !*c;
                    }
                }
                true
            }
            KeyCode::Enter
                if question.allow_other
                    && self.multiselect_cursor == other_idx
                    && !self.multiselect_typing_other =>
            {
                self.multiselect_typing_other = true;
                true
            }
            KeyCode::Char(c)
                if self.multiselect_typing_other
                    && !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.multiselect_other_text.push(c);
                true
            }
            KeyCode::Char(c)
                if question.allow_other
                    && !c.is_control()
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                    && self.multiselect_cursor == other_idx
                    && !self.multiselect_typing_other =>
            {
                // Start typing immediately when user types on "Other" — no need to press Enter first
                self.multiselect_typing_other = true;
                self.multiselect_other_text.push(c);
                true
            }
            KeyCode::Backspace if self.multiselect_typing_other => {
                self.multiselect_other_text.pop();
                true
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(5);
                true
            }
            KeyCode::PageDown => {
                self.scroll_offset = self.scroll_offset.saturating_add(5);
                true
            }
            _ => false,
        }
    }

    fn handle_text_input_key_view_local(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char(c)
                if !c.is_control() && !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.text_input.insert(self.text_input_cursor, c);
                self.text_input_cursor += c.len_utf8();
                true
            }
            KeyCode::Backspace if self.text_input_cursor > 0 => {
                let prev = prev_char_boundary(&self.text_input, self.text_input_cursor);
                self.text_input_cursor = prev;
                self.text_input.remove(self.text_input_cursor);
                true
            }
            KeyCode::Left => {
                self.text_input_cursor =
                    prev_char_boundary(&self.text_input, self.text_input_cursor);
                true
            }
            KeyCode::Right => {
                self.text_input_cursor =
                    advance_cursor_by_char(&self.text_input, self.text_input_cursor);
                true
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(5);
                true
            }
            KeyCode::PageDown => {
                self.scroll_offset = self.scroll_offset.saturating_add(5);
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyEventKind, KeyModifiers};

    use super::*;

    #[test]
    fn feature_input_ctrl_c_not_inserted_as_text() {
        let mut vs = ViewState::new();
        vs.on_mode_changed(&AppMode::FeatureInput);
        let ctrl_c = KeyEvent::new_with_kind(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        );
        assert!(
            !vs.handle_key_view_local(ctrl_c, &AppMode::FeatureInput, 0),
            "Ctrl+C must not be consumed as text; key_map handles Quit"
        );
        assert!(vs.feature_input.is_empty());
    }

    #[test]
    fn view_state_default() {
        let vs = ViewState::new();
        assert_eq!(vs.scroll_offset, 0);
        assert!(vs.feature_input.is_empty());
        assert_eq!(vs.feature_cursor, 0);
    }

    #[test]
    fn view_state_on_mode_changed_feature_input() {
        let mut vs = ViewState::new();
        vs.feature_input = "old".to_string();
        vs.on_mode_changed(&AppMode::FeatureInput);
        assert!(vs.feature_input.is_empty());
        assert_eq!(vs.feature_cursor, 0);
    }

    #[test]
    fn feature_input_handles_multi_byte_utf8() {
        let mut vs = ViewState::new();
        vs.on_mode_changed(&AppMode::FeatureInput);
        // Emoji is 4 bytes in UTF-8; cursor was incremented by 1 before fix, causing panic on next insert
        let emoji = KeyEvent::new(KeyCode::Char('😀'), KeyModifiers::empty());
        let a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty());
        assert!(vs.handle_key_view_local(emoji, &AppMode::FeatureInput, 0));
        assert_eq!(vs.feature_input, "😀");
        assert_eq!(vs.feature_cursor, 4); // byte index
        assert!(vs.handle_key_view_local(a, &AppMode::FeatureInput, 0));
        assert_eq!(vs.feature_input, "😀a");
        // Backspace removes 'a', Left moves to start of emoji
        let backspace = KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty());
        assert!(vs.handle_key_view_local(backspace, &AppMode::FeatureInput, 0));
        assert_eq!(vs.feature_input, "😀");
        let left = KeyEvent::new(KeyCode::Left, KeyModifiers::empty());
        assert!(vs.handle_key_view_local(left, &AppMode::FeatureInput, 0));
        assert_eq!(vs.feature_cursor, 0);
    }

    #[test]
    fn space_key_pages_down_in_markdown_viewer() {
        let mut vs = ViewState::new();
        let mode = AppMode::MarkdownViewer {
            content: "test content".to_string(),
        };
        let space = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty());
        vs.handle_key_view_local(space, &mode, 0);
        assert_eq!(vs.markdown_scroll_offset, 10);
    }

    #[test]
    fn down_arrow_navigates_buttons_when_at_end() {
        let mut vs = ViewState::new();
        vs.markdown_at_end = true;
        let mode = AppMode::MarkdownViewer {
            content: "test content".to_string(),
        };
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
        vs.handle_key_view_local(down, &mode, 0);
        assert_eq!(vs.markdown_end_button_selected, 1);
    }

    #[test]
    fn up_arrow_navigates_buttons_when_at_end() {
        let mut vs = ViewState::new();
        vs.markdown_at_end = true;
        vs.markdown_end_button_selected = 1;
        let mode = AppMode::MarkdownViewer {
            content: "test content".to_string(),
        };
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::empty());
        vs.handle_key_view_local(up, &mode, 0);
        assert_eq!(vs.markdown_end_button_selected, 0);
    }

    #[test]
    fn test_error_recovery_view_state_reset() {
        let mut vs = ViewState::new();
        vs.error_recovery_selected = 1;
        let mode = AppMode::ErrorRecovery {
            error_message: "failed".to_string(),
        };
        vs.on_mode_changed(&mode);
        assert_eq!(vs.error_recovery_selected, 0);
    }

    #[test]
    fn test_error_recovery_up_down_three_options() {
        let mut vs = ViewState::new();
        let mode = AppMode::ErrorRecovery {
            error_message: "failed".to_string(),
        };
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::empty());

        // Start at 0 (Resume), Down → 1 (Continue with agent)
        vs.handle_key_view_local(down, &mode, 0);
        assert_eq!(vs.error_recovery_selected, 1);

        // Down → 2 (Exit)
        vs.handle_key_view_local(down, &mode, 0);
        assert_eq!(vs.error_recovery_selected, 2);

        // Down → wraps to 0
        vs.handle_key_view_local(down, &mode, 0);
        assert_eq!(vs.error_recovery_selected, 0);

        // Up from 0 → wraps to 2
        vs.handle_key_view_local(up, &mode, 0);
        assert_eq!(vs.error_recovery_selected, 2);

        // Up from 2 → 1
        vs.handle_key_view_local(up, &mode, 0);
        assert_eq!(vs.error_recovery_selected, 1);

        // Up from 1 → 0
        vs.handle_key_view_local(up, &mode, 0);
        assert_eq!(vs.error_recovery_selected, 0);
    }
}
