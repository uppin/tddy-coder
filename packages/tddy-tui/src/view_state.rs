//! View-local state: scroll, text buffers, selection cursor.
//!
//! The Presenter owns application state; the View owns this view-local state
//! (editing buffers, cursor positions, scroll offset).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tddy_core::AppMode;
use tddy_core::ClarificationQuestion;
use tddy_core::PresenterState;

use crate::feature_input_buffer::FeatureInputBuffer;

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
    /// Feature input buffer (FeatureInput mode): compact `/skill` in the UI; submit expands for agents.
    pub feature_edit: FeatureInputBuffer,
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
    /// Selected option in DocumentReview mode (0=View, 1=Approve, 2=Refine).
    pub document_review_selected: usize,
    /// Scroll offset for MarkdownViewer mode.
    pub markdown_scroll_offset: usize,
    /// True when the MarkdownViewer has scrolled to the end of the document.
    pub markdown_at_end: bool,
    /// Index of the selected inline button at end of document (0=Approve, 1=Refine).
    pub markdown_end_button_selected: usize,
    /// Prompt-bar buffer while [`AppMode::MarkdownViewer`] + presenter `plan_refinement_pending`.
    pub plan_refinement_input: String,
    /// Byte cursor into [`Self::plan_refinement_input`].
    pub plan_refinement_cursor: usize,
    /// Selected option index in ErrorRecovery mode (0=Resume, 1=Exit).
    pub error_recovery_selected: usize,
    /// Frame counter for the spinner animation.
    pub spinner_tick: usize,
    /// Last Select question identity — used to avoid clearing Other-typing state on highlight-only ModeChanged.
    last_select_identity: Option<(String, String)>,
    /// Stable key for the current [`AppMode`] variant + question identity (status bar freeze boundaries).
    status_bar_mode_identity_cache: Option<u64>,
    /// Snapshot of `goal_start_time.elapsed()` when entering clarification wait while a goal row is shown.
    pub(crate) frozen_goal_elapsed_for_status_bar: Option<Duration>,
    /// Wall-clock anchor for 1 Hz idle dot (· ↔ •) in user-wait modes.
    pub(crate) idle_dot_animation_anchor: Option<Instant>,
    /// Feature-prompt slash menu (skills + `/recipe`) is visible.
    pub feature_slash_open: bool,
    /// Menu rows from [`tddy_core::slash_menu_entries`].
    pub feature_slash_entries: Vec<tddy_core::SlashMenuEntry>,
    pub feature_slash_selected: usize,
    /// Byte index of the `/` that opened [`Self::feature_slash_open`].
    pub feature_slash_trigger_byte: usize,
    /// After accepting `/recipe` in the slash menu, event loop sends [`tddy_core::UserIntent::FeatureSlashBuiltinRecipe`].
    pending_feature_slash_builtin_recipe_intent: bool,
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

/// True when `/` at `slash_byte_idx` starts a slash command (line start or after whitespace).
fn slash_starts_command(s: &str, slash_byte_idx: usize) -> bool {
    if slash_byte_idx >= s.len() || !s.is_char_boundary(slash_byte_idx) {
        return false;
    }
    if s.as_bytes().get(slash_byte_idx) != Some(&b'/') {
        return false;
    }
    if slash_byte_idx == 0 {
        return true;
    }
    s[..slash_byte_idx]
        .chars()
        .next_back()
        .is_some_and(|c| c.is_whitespace())
}

/// Hash [`AppMode`] so distinct clarification questions get distinct status-bar animation state.
fn status_bar_mode_identity_key(mode: &AppMode) -> u64 {
    let mut h = DefaultHasher::new();
    std::mem::discriminant(mode).hash(&mut h);
    match mode {
        AppMode::Select {
            question,
            question_index,
            ..
        } => {
            question_index.hash(&mut h);
            question.header.hash(&mut h);
            question.question.hash(&mut h);
        }
        AppMode::MultiSelect {
            question,
            question_index,
            ..
        } => {
            question_index.hash(&mut h);
            question.header.hash(&mut h);
            question.question.hash(&mut h);
        }
        AppMode::TextInput { prompt } => {
            prompt.hash(&mut h);
        }
        _ => {}
    }
    h.finish()
}

impl ViewState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update frozen goal elapsed and idle-dot anchor when [`PresenterState::mode`] identity changes.
    /// Call at the start of each frame before building status bar text (see [`crate::render::draw`]).
    pub fn sync_status_bar_with_presenter(&mut self, state: &PresenterState) {
        let key = status_bar_mode_identity_key(&state.mode);
        if Some(key) != self.status_bar_mode_identity_cache {
            log::debug!(
                "view_state: status bar mode identity {:?} -> {:?}",
                self.status_bar_mode_identity_cache,
                Some(key)
            );
            self.status_bar_mode_identity_cache = Some(key);
            let user_wait = matches!(
                &state.mode,
                AppMode::Select { .. } | AppMode::MultiSelect { .. } | AppMode::TextInput { .. }
            );
            let active_goal_row = state.current_goal.is_some() && state.current_state.is_some();
            if user_wait && active_goal_row {
                let elapsed = state.goal_start_time.elapsed();
                self.frozen_goal_elapsed_for_status_bar = Some(elapsed);
                self.idle_dot_animation_anchor = Some(Instant::now());
                log::info!(
                    "status bar: clarification wait — froze goal elapsed display at {:?}, idle dot anchor set",
                    elapsed
                );
            } else {
                self.frozen_goal_elapsed_for_status_bar = None;
                self.idle_dot_animation_anchor = None;
                log::debug!("status bar: cleared frozen elapsed / idle anchor (not user-wait or no goal row)");
            }
        }
    }

    /// Reset view state when entering a new mode. Call from TuiView when mode changes.
    pub fn on_mode_changed(&mut self, mode: &AppMode) {
        if !matches!(mode, AppMode::Select { .. }) {
            self.last_select_identity = None;
        }
        match mode {
            AppMode::FeatureInput => {
                self.feature_edit.clear();
                self.close_feature_slash_menu_clear();
                self.pending_feature_slash_builtin_recipe_intent = false;
            }
            AppMode::Select {
                question,
                initial_selected,
                ..
            } => {
                let id = (question.header.clone(), question.question.clone());
                let same_question = self.last_select_identity.as_ref() == Some(&id);
                if !same_question {
                    self.select_other_text.clear();
                    self.select_typing_other = false;
                }
                self.last_select_identity = Some(id);
                self.select_selected = *initial_selected;
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
            AppMode::DocumentReview { .. } => {
                self.document_review_selected = 0;
            }
            AppMode::MarkdownViewer { .. } => {
                self.markdown_scroll_offset = 0;
                self.markdown_at_end = false;
                self.markdown_end_button_selected = 0;
                self.plan_refinement_input.clear();
                self.plan_refinement_cursor = 0;
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
        plan_refinement_pending: bool,
        skills_project_root: Option<&Path>,
    ) -> bool {
        if key.kind != KeyEventKind::Press {
            return false;
        }

        match mode {
            AppMode::FeatureInput => self.handle_feature_input_key(key, skills_project_root),
            AppMode::Running => self.handle_running_key_view_local(key, inbox_len),
            AppMode::DocumentReview { .. } => self.handle_document_review_key_view_local(key),
            AppMode::MarkdownViewer { .. } => {
                self.handle_markdown_viewer_key_view_local(key, plan_refinement_pending)
            }
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

    fn handle_document_review_key_view_local(&mut self, key: KeyEvent) -> bool {
        const OPTIONS: usize = 3; // View, Approve, Refine
        match key.code {
            KeyCode::Up => {
                self.document_review_selected = if self.document_review_selected == 0 {
                    OPTIONS - 1
                } else {
                    self.document_review_selected - 1
                };
                true
            }
            KeyCode::Down => {
                self.document_review_selected = if self.document_review_selected >= OPTIONS - 1 {
                    0
                } else {
                    self.document_review_selected + 1
                };
                true
            }
            _ => false,
        }
    }

    fn handle_plan_refinement_prompt_key_local(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char(c)
                if !c.is_control()
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                self.plan_refinement_input
                    .insert(self.plan_refinement_cursor, c);
                self.plan_refinement_cursor += c.len_utf8();
                true
            }
            KeyCode::Backspace if self.plan_refinement_cursor > 0 => {
                let prev =
                    prev_char_boundary(&self.plan_refinement_input, self.plan_refinement_cursor);
                self.plan_refinement_cursor = prev;
                self.plan_refinement_input
                    .remove(self.plan_refinement_cursor);
                true
            }
            KeyCode::Left => {
                self.plan_refinement_cursor =
                    prev_char_boundary(&self.plan_refinement_input, self.plan_refinement_cursor);
                true
            }
            KeyCode::Right => {
                self.plan_refinement_cursor = advance_cursor_by_char(
                    &self.plan_refinement_input,
                    self.plan_refinement_cursor,
                );
                true
            }
            _ => false,
        }
    }

    fn handle_markdown_viewer_key_view_local(
        &mut self,
        key: KeyEvent,
        plan_refinement_pending: bool,
    ) -> bool {
        if plan_refinement_pending {
            log::debug!("markdown viewer: key routed to plan refinement prompt");
            return self.handle_plan_refinement_prompt_key_local(key);
        }

        match key.code {
            KeyCode::Backspace | KeyCode::Left | KeyCode::Right => {
                if self.handle_plan_refinement_prompt_key_local(key) {
                    return true;
                }
            }
            KeyCode::Char(c)
                if !c.is_control()
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                if self.handle_plan_refinement_prompt_key_local(key) {
                    return true;
                }
            }
            _ => {}
        }

        match key.code {
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
            _ => false,
        }
    }

    /// Screen lines reserved below the activity log for the slash menu (header + rows + hint).
    pub fn feature_slash_dynamic_height(&self) -> u16 {
        if !self.feature_slash_open || self.feature_slash_entries.is_empty() {
            return 0;
        }
        let n = (self.feature_slash_entries.len() as u16).min(12);
        1u16.saturating_add(n).saturating_add(1)
    }

    pub fn take_pending_feature_slash_builtin_recipe_intent(&mut self) -> bool {
        std::mem::take(&mut self.pending_feature_slash_builtin_recipe_intent)
    }

    fn close_feature_slash_menu_clear(&mut self) {
        self.feature_slash_open = false;
        self.feature_slash_entries.clear();
        self.feature_slash_selected = 0;
    }

    fn open_feature_slash_menu(&mut self, trigger_byte: usize, skills_project_root: Option<&Path>) {
        let root = skills_project_root.unwrap_or_else(|| Path::new("."));
        self.feature_slash_trigger_byte = trigger_byte;
        self.feature_slash_entries = tddy_core::slash_menu_entries(root);
        self.feature_slash_selected = 0;
        self.feature_slash_open = true;
    }

    fn strip_slash_suffix_after_trigger(&mut self) {
        let t = self
            .feature_slash_trigger_byte
            .min(self.feature_edit.display().len());
        self.feature_edit.truncate_at_slash_trigger(t);
        self.feature_edit.cursor = self.feature_edit.cursor.min(t);
    }

    fn accept_feature_slash_skill(&mut self, skill_name: &str, project_root: &Path) {
        let skill_md = project_root
            .join(tddy_core::AGENTS_SKILLS_DIR)
            .join(skill_name)
            .join("SKILL.md");
        if skill_md.is_file() {
            let trigger = self.feature_slash_trigger_byte;
            self.feature_edit.accept_skill_token(skill_name, trigger);
            self.close_feature_slash_menu_clear();
        } else {
            log::warn!("feature slash: missing skill file {}", skill_md.display());
            self.strip_slash_suffix_after_trigger();
            self.close_feature_slash_menu_clear();
        }
    }

    fn handle_feature_slash_menu_key(
        &mut self,
        key: KeyEvent,
        skills_project_root: Option<&Path>,
    ) -> bool {
        let root = skills_project_root.unwrap_or_else(|| Path::new("."));
        match key.code {
            KeyCode::Esc => {
                self.strip_slash_suffix_after_trigger();
                self.close_feature_slash_menu_clear();
                true
            }
            KeyCode::Up => {
                if self.feature_slash_selected > 0 {
                    self.feature_slash_selected -= 1;
                } else {
                    self.feature_slash_selected =
                        self.feature_slash_entries.len().saturating_sub(1);
                }
                true
            }
            KeyCode::Down => {
                let n = self.feature_slash_entries.len();
                if n > 0 {
                    self.feature_slash_selected = (self.feature_slash_selected + 1) % n;
                }
                true
            }
            KeyCode::Enter => {
                if self.feature_slash_entries.is_empty() {
                    return true;
                }
                match &self.feature_slash_entries[self.feature_slash_selected] {
                    tddy_core::SlashMenuEntry::BuiltinRecipe => {
                        self.strip_slash_suffix_after_trigger();
                        self.close_feature_slash_menu_clear();
                        self.pending_feature_slash_builtin_recipe_intent = true;
                    }
                    tddy_core::SlashMenuEntry::Skill { name, .. } => {
                        let name = name.clone();
                        self.accept_feature_slash_skill(&name, root);
                    }
                }
                true
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Backspace => {
                self.close_feature_slash_menu_clear();
                false
            }
            KeyCode::Char(c)
                if !c.is_control() && !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.close_feature_slash_menu_clear();
                let insert_at = self.feature_edit.cursor;
                self.feature_edit.insert_char(c);
                if c == '/' && slash_starts_command(&self.feature_edit.display(), insert_at) {
                    self.open_feature_slash_menu(insert_at, skills_project_root);
                }
                true
            }
            _ => {
                self.close_feature_slash_menu_clear();
                false
            }
        }
    }

    fn handle_feature_input_key(
        &mut self,
        key: KeyEvent,
        skills_project_root: Option<&Path>,
    ) -> bool {
        if self.feature_slash_open {
            return self.handle_feature_slash_menu_key(key, skills_project_root);
        }
        match key.code {
            // Ctrl+letter must not be inserted as text (e.g. Ctrl+C = Quit). Real TUI skips
            // handle_key_view_local for Ctrl+C; VirtualTui relies on this guard + key_map.
            KeyCode::Char(c)
                if !c.is_control() && !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                let insert_at = self.feature_edit.cursor;
                self.feature_edit.insert_char(c);
                if c == '/' && slash_starts_command(&self.feature_edit.display(), insert_at) {
                    self.open_feature_slash_menu(insert_at, skills_project_root);
                }
                true
            }
            KeyCode::Backspace => {
                if self.feature_edit.backspace() {
                    return true;
                }
                false
            }
            KeyCode::Left => {
                self.feature_edit.move_left();
                true
            }
            KeyCode::Right => {
                self.feature_edit.move_right();
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
    use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

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
            !vs.handle_key_view_local(ctrl_c, &AppMode::FeatureInput, 0, false, None),
            "Ctrl+C must not be consumed as text; key_map handles Quit"
        );
        assert!(vs.feature_edit.display().is_empty());
    }

    #[test]
    fn view_state_default() {
        let vs = ViewState::new();
        assert_eq!(vs.scroll_offset, 0);
        assert!(vs.feature_edit.display().is_empty());
        assert_eq!(vs.feature_edit.cursor, 0);
    }

    #[test]
    fn view_state_on_mode_changed_feature_input() {
        let mut vs = ViewState::new();
        vs.feature_edit.set_plain_text("old");
        vs.on_mode_changed(&AppMode::FeatureInput);
        assert!(vs.feature_edit.display().is_empty());
        assert_eq!(vs.feature_edit.cursor, 0);
    }

    #[test]
    fn feature_input_handles_multi_byte_utf8() {
        let mut vs = ViewState::new();
        vs.on_mode_changed(&AppMode::FeatureInput);
        // Emoji is 4 bytes in UTF-8; cursor was incremented by 1 before fix, causing panic on next insert
        let emoji = KeyEvent::new(KeyCode::Char('😀'), KeyModifiers::empty());
        let a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty());
        assert!(vs.handle_key_view_local(emoji, &AppMode::FeatureInput, 0, false, None));
        assert_eq!(vs.feature_edit.display(), "😀");
        assert_eq!(vs.feature_edit.cursor, 4); // byte index
        assert!(vs.handle_key_view_local(a, &AppMode::FeatureInput, 0, false, None));
        assert_eq!(vs.feature_edit.display(), "😀a");
        // Backspace removes 'a', Left moves to start of emoji
        let backspace = KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty());
        assert!(vs.handle_key_view_local(backspace, &AppMode::FeatureInput, 0, false, None));
        assert_eq!(vs.feature_edit.display(), "😀");
        let left = KeyEvent::new(KeyCode::Left, KeyModifiers::empty());
        assert!(vs.handle_key_view_local(left, &AppMode::FeatureInput, 0, false, None));
        assert_eq!(vs.feature_edit.cursor, 0);
    }

    #[test]
    fn page_down_scrolls_markdown_viewer_body() {
        let mut vs = ViewState::new();
        let mode = AppMode::MarkdownViewer {
            content: "test content".to_string(),
        };
        let pg = KeyEvent::new(KeyCode::PageDown, KeyModifiers::empty());
        vs.handle_key_view_local(pg, &mode, 0, false, None);
        assert_eq!(vs.markdown_scroll_offset, 10);
    }

    #[test]
    fn markdown_viewer_inserts_plain_a_into_refinement_buffer() {
        let mut vs = ViewState::new();
        let mode = AppMode::MarkdownViewer {
            content: "# p".to_string(),
        };
        let key = KeyEvent::new_with_kind(
            KeyCode::Char('a'),
            KeyModifiers::empty(),
            KeyEventKind::Press,
        );
        assert!(vs.handle_key_view_local(key, &mode, 0, false, None));
        assert_eq!(vs.plan_refinement_input, "a");
    }

    #[test]
    fn markdown_viewer_inserts_plain_r_into_refinement_buffer() {
        let mut vs = ViewState::new();
        let mode = AppMode::MarkdownViewer {
            content: "# p".to_string(),
        };
        let key = KeyEvent::new_with_kind(
            KeyCode::Char('r'),
            KeyModifiers::empty(),
            KeyEventKind::Press,
        );
        assert!(vs.handle_key_view_local(key, &mode, 0, false, None));
        assert_eq!(vs.plan_refinement_input, "r");
    }

    #[test]
    fn markdown_viewer_accepts_text_for_refinement_while_prd_stays_open() {
        let mut vs = ViewState::new();
        let mode = AppMode::MarkdownViewer {
            content: "# My PRD".to_string(),
        };
        let key = KeyEvent::new_with_kind(
            KeyCode::Char('f'),
            KeyModifiers::empty(),
            KeyEventKind::Press,
        );
        assert!(
            vs.handle_key_view_local(key, &mode, 0, false, None),
            "printable keys should update the refinement buffer while the PRD remains visible"
        );
        assert_eq!(vs.plan_refinement_input, "f");
    }

    #[test]
    fn markdown_viewer_backspace_edits_refinement_buffer_without_pending_flag() {
        let mut vs = ViewState::new();
        vs.plan_refinement_input = "hi".to_string();
        vs.plan_refinement_cursor = 2;
        let mode = AppMode::MarkdownViewer {
            content: "# p".to_string(),
        };
        let bs = KeyEvent::new_with_kind(
            KeyCode::Backspace,
            KeyModifiers::empty(),
            KeyEventKind::Press,
        );
        assert!(vs.handle_key_view_local(bs, &mode, 0, false, None));
        assert_eq!(vs.plan_refinement_input, "h");
        assert_eq!(vs.plan_refinement_cursor, 1);
    }

    #[test]
    fn markdown_viewer_space_inserts_into_refinement_buffer_without_pending_flag() {
        let mut vs = ViewState::new();
        vs.plan_refinement_input = "a".to_string();
        vs.plan_refinement_cursor = 1;
        let mode = AppMode::MarkdownViewer {
            content: "# p".to_string(),
        };
        let sp = KeyEvent::new_with_kind(
            KeyCode::Char(' '),
            KeyModifiers::empty(),
            KeyEventKind::Press,
        );
        assert!(vs.handle_key_view_local(sp, &mode, 0, false, None));
        assert_eq!(vs.plan_refinement_input, "a ");
        assert_eq!(vs.plan_refinement_cursor, 2);
        assert_eq!(vs.markdown_scroll_offset, 0);
    }

    #[test]
    fn markdown_viewer_scroll_does_not_use_space_or_line_arrows() {
        let mut vs = ViewState::new();
        vs.markdown_at_end = false;
        vs.markdown_scroll_offset = 100;
        let mode = AppMode::MarkdownViewer {
            content: "# p".to_string(),
        };
        let space = KeyEvent::new_with_kind(
            KeyCode::Char(' '),
            KeyModifiers::empty(),
            KeyEventKind::Press,
        );
        assert!(vs.handle_key_view_local(space, &mode, 0, false, None));
        assert_eq!(
            vs.markdown_scroll_offset, 100,
            "space must not scroll the plan body"
        );
        vs.plan_refinement_input.clear();
        vs.plan_refinement_cursor = 0;
        let up = KeyEvent::new_with_kind(KeyCode::Up, KeyModifiers::empty(), KeyEventKind::Press);
        vs.handle_key_view_local(up, &mode, 0, false, None);
        assert_eq!(
            vs.markdown_scroll_offset, 100,
            "Up must not line-scroll; use PgUp/PgDown only"
        );
        let down =
            KeyEvent::new_with_kind(KeyCode::Down, KeyModifiers::empty(), KeyEventKind::Press);
        vs.handle_key_view_local(down, &mode, 0, false, None);
        assert_eq!(
            vs.markdown_scroll_offset, 100,
            "Down must not line-scroll; use PgUp/PgDown only"
        );
    }

    #[test]
    fn down_arrow_navigates_buttons_when_at_end() {
        let mut vs = ViewState::new();
        vs.markdown_at_end = true;
        let mode = AppMode::MarkdownViewer {
            content: "test content".to_string(),
        };
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
        vs.handle_key_view_local(down, &mode, 0, false, None);
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
        vs.handle_key_view_local(up, &mode, 0, false, None);
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
        vs.handle_key_view_local(down, &mode, 0, false, None);
        assert_eq!(vs.error_recovery_selected, 1);

        // Down → 2 (Exit)
        vs.handle_key_view_local(down, &mode, 0, false, None);
        assert_eq!(vs.error_recovery_selected, 2);

        // Down → wraps to 0
        vs.handle_key_view_local(down, &mode, 0, false, None);
        assert_eq!(vs.error_recovery_selected, 0);

        // Up from 0 → wraps to 2
        vs.handle_key_view_local(up, &mode, 0, false, None);
        assert_eq!(vs.error_recovery_selected, 2);

        // Up from 2 → 1
        vs.handle_key_view_local(up, &mode, 0, false, None);
        assert_eq!(vs.error_recovery_selected, 1);

        // Up from 1 → 0
        vs.handle_key_view_local(up, &mode, 0, false, None);
        assert_eq!(vs.error_recovery_selected, 0);
    }

    #[test]
    fn feature_slash_opens_at_line_start_lists_recipe() {
        use std::fs;
        let mut vs = ViewState::new();
        let root = std::env::temp_dir().join(format!("tddy-slash-ui-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(".agents/skills")).expect("mkdir");
        vs.on_mode_changed(&AppMode::FeatureInput);
        let slash = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::empty());
        assert!(vs.handle_key_view_local(
            slash,
            &AppMode::FeatureInput,
            0,
            false,
            Some(root.as_path())
        ));
        assert!(vs.feature_slash_open);
        assert!(
            vs.feature_slash_entries
                .iter()
                .any(|e| matches!(e, tddy_core::SlashMenuEntry::BuiltinRecipe)),
            "expected /recipe builtin, got {:?}",
            vs.feature_slash_entries
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn feature_slash_mid_word_does_not_open() {
        let mut vs = ViewState::new();
        vs.on_mode_changed(&AppMode::FeatureInput);
        vs.handle_key_view_local(
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty()),
            &AppMode::FeatureInput,
            0,
            false,
            None,
        );
        vs.handle_key_view_local(
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::empty()),
            &AppMode::FeatureInput,
            0,
            false,
            None,
        );
        assert!(!vs.feature_slash_open);
    }
}
