//! Application state for the TUI: modes, activity log, status tracking.

use std::time::Instant;
use tddy_core::{ClarificationQuestion, ProgressEvent};

use crate::tui::event::TuiEvent;
use crate::tui::input::{handle_multiselect_key, handle_select_key, handle_text_input_key};

/// Instruction prefix prepended to dequeued inbox prompts so the agent knows the request was queued.
pub const QUEUED_INSTRUCTION_PREFIX: &str =
    "[QUEUED] The following prompt was queued while you were busy. Please address it:\n\n";

/// Which sub-element has focus when the inbox is visible during Running mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboxFocus {
    /// Focus is on the running-mode input field (default).
    None,
    /// Focus is on the inbox list; user can navigate with Up/Down.
    List,
    /// User is editing the selected inbox item in-place.
    Editing,
}

/// The current interaction mode of the TUI.
#[derive(Debug, Clone)]
pub enum AppMode {
    /// Waiting for the user to type a feature description.
    FeatureInput { input: String, cursor: usize },
    /// Workflow is running; no user input needed.
    Running,
    /// Presenting a single-select clarification question.
    Select {
        question: ClarificationQuestion,
        /// Currently highlighted option index (0-based, includes "Other" as last).
        selected: usize,
        /// Text typed when "Other (type your own)" is selected.
        other_text: String,
        /// True when the user has selected "Other" and is typing a custom answer.
        typing_other: bool,
    },
    /// Presenting a multi-select clarification question.
    MultiSelect {
        question: ClarificationQuestion,
        /// Currently highlighted option index.
        cursor: usize,
        /// Checked state per option (indices match original options + 1 for "Other").
        checked: Vec<bool>,
        /// Text typed for the "Other" option.
        other_text: String,
        /// True when the user is typing a custom answer for the "Other" option.
        typing_other: bool,
    },
    /// Free-form text input (question with no predefined options).
    TextInput {
        prompt: String,
        input: String,
        cursor: usize,
    },
    /// User must choose Run or Skip for demo before green goal proceeds.
    DemoPrompt,
    /// Workflow complete; showing final result.
    Done,
}

/// Kind of entry in the activity log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityKind {
    ToolUse,
    TaskStarted,
    TaskProgress,
    StateChange,
    Info,
    /// Raw agent output (assistant text, tool results).
    AgentOutput,
}

/// A single entry in the scrollable activity log.
#[derive(Debug, Clone)]
pub struct ActivityEntry {
    pub text: String,
    pub kind: ActivityKind,
}

/// Top-level application state for the TUI event loop.
pub struct AppState {
    pub agent: String,
    pub model: String,
    pub mode: AppMode,
    pub current_goal: Option<String>,
    pub current_state: Option<String>,
    pub goal_start_time: Instant,
    pub activity_log: Vec<ActivityEntry>,
    pub auto_scroll: bool,
    /// Manual scroll offset (lines from top). Used when auto_scroll is false.
    pub scroll_offset: usize,
    pub should_quit: bool,
    /// All questions in the current clarification round.
    pub pending_questions: Vec<ClarificationQuestion>,
    /// Index into `pending_questions` of the question currently being shown.
    pub current_question_index: usize,
    /// Answers collected so far in this clarification round (one per question answered).
    pub collected_answers: Vec<String>,
    /// When user submits feature input (Enter in FeatureInput mode), stored here for the event loop to send.
    pub submitted_feature_input: Option<String>,
    /// Buffers agent output chunks until a newline is received. Segments don't line-break unless \n is in the chunk.
    pub agent_output_buffer: String,
    /// Queued prompts the user typed while the agent was busy.
    pub inbox: Vec<String>,
    /// Currently selected item index in the inbox list.
    pub inbox_cursor: usize,
    /// Which sub-element has focus when inbox is visible.
    pub inbox_focus: InboxFocus,
    /// Buffer for editing an inbox item in-place.
    pub inbox_edit_buffer: String,
    /// Text the user is typing in the prompt bar during Running mode.
    pub running_input: String,
    /// Cursor position within `running_input`.
    pub running_cursor: usize,
    /// When user chooses Run/Skip in DemoPrompt mode, set for event loop to send.
    pub demo_choice_to_send: Option<String>,
}

impl AppState {
    /// Create a new AppState in FeatureInput mode.
    pub fn new(agent: impl Into<String>, model: impl Into<String>) -> Self {
        AppState {
            agent: agent.into(),
            model: model.into(),
            mode: AppMode::FeatureInput {
                input: String::new(),
                cursor: 0,
            },
            current_goal: None,
            current_state: None,
            goal_start_time: Instant::now(),
            activity_log: Vec::new(),
            auto_scroll: true,
            scroll_offset: 0,
            should_quit: false,
            pending_questions: Vec::new(),
            current_question_index: 0,
            collected_answers: Vec::new(),
            submitted_feature_input: None,
            agent_output_buffer: String::new(),
            inbox: Vec::new(),
            inbox_cursor: 0,
            inbox_focus: InboxFocus::None,
            inbox_edit_buffer: String::new(),
            running_input: String::new(),
            running_cursor: 0,
            demo_choice_to_send: None,
        }
    }

    /// Process a TuiEvent and update state accordingly.
    pub fn handle_event(&mut self, event: TuiEvent) {
        match event {
            TuiEvent::Key(key) => self.handle_key(key),
            TuiEvent::Progress(ev) => self.handle_progress(ev),
            TuiEvent::ClarificationNeeded { questions } => {
                self.flush_agent_output_buffer();
                self.pending_questions = questions;
                self.current_question_index = 0;
                self.collected_answers.clear();
                self.advance_to_next_question();
            }
            TuiEvent::WorkflowComplete(result) => {
                self.flush_agent_output_buffer();
                self.handle_workflow_complete(result);
            }
            TuiEvent::GoalStarted(goal) => {
                self.flush_agent_output_buffer();
                self.current_goal = Some(goal);
                self.goal_start_time = Instant::now();
            }
            TuiEvent::StateChange { from, to } => {
                self.current_state = Some(to.clone());
                self.activity_log.push(ActivityEntry {
                    text: format!("State: {} → {}", from, to),
                    kind: ActivityKind::StateChange,
                });
            }
            TuiEvent::AgentOutput(text) => {
                for part in text.split_inclusive('\n') {
                    if part.ends_with('\n') {
                        self.agent_output_buffer
                            .push_str(part.trim_end_matches('\n'));
                        let line = std::mem::take(&mut self.agent_output_buffer);
                        if !line.is_empty() {
                            self.activity_log.push(ActivityEntry {
                                text: line,
                                kind: ActivityKind::AgentOutput,
                            });
                        }
                    } else {
                        self.agent_output_buffer.push_str(part);
                    }
                }
            }
            TuiEvent::Resize(_, _) => {}
            TuiEvent::Scroll { delta } => self.handle_scroll(delta),
            TuiEvent::DemoPrompt => {
                self.flush_agent_output_buffer();
                self.mode = AppMode::DemoPrompt;
            }
        }
    }

    fn flush_agent_output_buffer(&mut self) {
        if !self.agent_output_buffer.is_empty() {
            let line = std::mem::take(&mut self.agent_output_buffer);
            self.activity_log.push(ActivityEntry {
                text: line,
                kind: ActivityKind::AgentOutput,
            });
        }
    }

    fn handle_scroll(&mut self, delta: i32) {
        let line_count = self.activity_log.len();
        if line_count == 0 {
            return;
        }
        self.auto_scroll = false;
        let current = self.scroll_offset as i32;
        // delta > 0 = scroll up (see earlier), delta < 0 = scroll down (see later)
        let max_offset = (line_count.saturating_sub(1)) as i32;
        let new_offset = (current + delta).clamp(0, max_offset);
        self.scroll_offset = new_offset as usize;
    }

    /// Return all collected clarification answers joined with '\n'.
    pub fn collect_answers(&self) -> String {
        self.collected_answers.join("\n")
    }

    /// Returns true when we've just finished answering all clarification questions
    /// and answers are ready to send to the workflow thread.
    pub fn clarification_answers_ready(&self) -> bool {
        !self.pending_questions.is_empty()
            && self.current_question_index >= self.pending_questions.len()
            && matches!(self.mode, AppMode::Running)
    }

    fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;

        // Activity log scroll: PageUp/PageDown work in any mode
        match key.code {
            KeyCode::PageUp => {
                self.handle_scroll(5);
                return;
            }
            KeyCode::PageDown => {
                self.handle_scroll(-5);
                return;
            }
            _ => {}
        }

        // Take ownership of mode to avoid borrow-checker conflicts when reassigning self.mode
        let current_mode = std::mem::replace(&mut self.mode, AppMode::Running);
        match current_mode {
            AppMode::FeatureInput { input, cursor } => match key.code {
                KeyCode::Char(c) => {
                    let mut s = input;
                    s.insert(cursor, c);
                    self.mode = AppMode::FeatureInput {
                        input: s,
                        cursor: cursor + 1,
                    };
                }
                KeyCode::Backspace if cursor > 0 => {
                    let mut s = input;
                    s.remove(cursor - 1);
                    self.mode = AppMode::FeatureInput {
                        input: s,
                        cursor: cursor - 1,
                    };
                }
                KeyCode::Enter if !input.is_empty() => {
                    self.submitted_feature_input = Some(input);
                    self.mode = AppMode::Running;
                }
                _ => {
                    self.mode = AppMode::FeatureInput { input, cursor };
                }
            },
            AppMode::Select {
                question,
                selected,
                other_text,
                typing_other,
            } => {
                let option_count = question.options.len();
                let other_idx = option_count;
                match key.code {
                    KeyCode::Enter if !typing_other && selected < option_count => {
                        let answer = question.options[selected].label.clone();
                        self.collected_answers.push(answer);
                        self.current_question_index += 1;
                        self.advance_to_next_question();
                    }
                    KeyCode::Enter if !typing_other && selected == other_idx => {
                        self.mode = AppMode::Select {
                            question,
                            selected,
                            other_text,
                            typing_other: true,
                        };
                    }
                    KeyCode::Enter if typing_other => {
                        let answer = other_text.clone();
                        self.collected_answers.push(answer);
                        self.current_question_index += 1;
                        self.advance_to_next_question();
                    }
                    _ => {
                        let mode = AppMode::Select {
                            question,
                            selected,
                            other_text,
                            typing_other,
                        };
                        self.mode = handle_select_key(mode, key);
                    }
                }
            }
            AppMode::MultiSelect {
                question,
                cursor,
                checked,
                other_text,
                typing_other,
            } => {
                let mode = AppMode::MultiSelect {
                    question,
                    cursor,
                    checked,
                    other_text,
                    typing_other,
                };
                let (new_mode, answer_opt) = handle_multiselect_key(mode, key);
                if let Some(answer) = answer_opt {
                    self.collected_answers.push(answer);
                    self.current_question_index += 1;
                    self.advance_to_next_question();
                } else {
                    self.mode = new_mode;
                }
            }
            AppMode::TextInput {
                prompt,
                input,
                cursor,
            } => {
                let (new_mode, submitted) = handle_text_input_key(prompt, input, cursor, key);
                if let Some(answer) = submitted {
                    self.collected_answers.push(answer);
                    self.current_question_index += 1;
                    self.advance_to_next_question();
                } else {
                    self.mode = new_mode;
                }
            }
            AppMode::Running => {
                self.handle_running_key(key);
            }
            AppMode::DemoPrompt => match key.code {
                KeyCode::Char('r') | KeyCode::Char('R') => {
                    self.demo_choice_to_send = Some("run".to_string());
                    self.mode = AppMode::Running;
                }
                KeyCode::Char('s') | KeyCode::Char('S') => {
                    self.demo_choice_to_send = Some("skip".to_string());
                    self.mode = AppMode::Running;
                }
                _ => self.mode = AppMode::DemoPrompt,
            },
            AppMode::Done => {
                self.mode = AppMode::Done;
            }
        }
    }

    /// Handle key events during Running mode: inbox input, navigation, edit, delete.
    fn handle_running_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;

        match self.inbox_focus {
            InboxFocus::None => match key.code {
                KeyCode::Char(c) => {
                    self.running_input.insert(self.running_cursor, c);
                    self.running_cursor += 1;
                }
                KeyCode::Backspace if self.running_cursor > 0 => {
                    self.running_cursor -= 1;
                    self.running_input.remove(self.running_cursor);
                }
                KeyCode::Enter if !self.running_input.is_empty() => {
                    let text = std::mem::take(&mut self.running_input);
                    self.running_cursor = 0;
                    self.inbox.push(text);
                }
                KeyCode::Up if self.running_input.is_empty() && !self.inbox.is_empty() => {
                    self.inbox_focus = InboxFocus::List;
                    self.inbox_cursor = self.inbox.len().saturating_sub(1);
                }
                _ => {}
            },
            InboxFocus::List => match key.code {
                KeyCode::Up => {
                    self.inbox_cursor = self.inbox_cursor.saturating_sub(1);
                }
                KeyCode::Down => {
                    let max = self.inbox.len().saturating_sub(1);
                    if self.inbox_cursor < max {
                        self.inbox_cursor += 1;
                    }
                }
                KeyCode::Char('E') => {
                    self.inbox_edit_buffer = self.inbox[self.inbox_cursor].clone();
                    self.inbox_focus = InboxFocus::Editing;
                }
                KeyCode::Char('D') => {
                    self.inbox.remove(self.inbox_cursor);
                    if self.inbox.is_empty() {
                        self.inbox_focus = InboxFocus::None;
                        self.inbox_cursor = 0;
                    } else if self.inbox_cursor >= self.inbox.len() {
                        self.inbox_cursor = self.inbox.len() - 1;
                    }
                }
                KeyCode::Esc => {
                    self.inbox_focus = InboxFocus::None;
                }
                _ => {}
            },
            InboxFocus::Editing => match key.code {
                KeyCode::Char(c) => {
                    self.inbox_edit_buffer.push(c);
                }
                KeyCode::Backspace => {
                    self.inbox_edit_buffer.pop();
                }
                KeyCode::Enter => {
                    self.inbox[self.inbox_cursor] = self.inbox_edit_buffer.clone();
                    self.inbox_edit_buffer.clear();
                    self.inbox_focus = InboxFocus::List;
                }
                KeyCode::Esc => {
                    self.inbox_edit_buffer.clear();
                    self.inbox_focus = InboxFocus::List;
                }
                _ => {}
            },
        }

        self.mode = AppMode::Running;
    }

    /// Handle WorkflowComplete: dequeue inbox if non-empty, otherwise transition to Done.
    fn handle_workflow_complete(&mut self, result: Result<String, String>) {
        if result.is_ok() && !self.inbox.is_empty() {
            let item = self.inbox.remove(0);
            let prefixed = format!("{}{}", QUEUED_INSTRUCTION_PREFIX, item);
            self.submitted_feature_input = Some(prefixed);
            self.mode = AppMode::Running;
        } else {
            self.mode = AppMode::Done;
        }
    }

    fn handle_progress(&mut self, event: ProgressEvent) {
        let entry = match event {
            ProgressEvent::ToolUse {
                name,
                detail: Some(d),
            } => ActivityEntry {
                text: format!("Tool: {} {}", name, d),
                kind: ActivityKind::ToolUse,
            },
            ProgressEvent::ToolUse { name, detail: None } => ActivityEntry {
                text: format!("Tool: {}", name),
                kind: ActivityKind::ToolUse,
            },
            ProgressEvent::TaskStarted { description } => ActivityEntry {
                text: description,
                kind: ActivityKind::TaskStarted,
            },
            ProgressEvent::TaskProgress {
                description,
                last_tool: _,
            } => ActivityEntry {
                text: description,
                kind: ActivityKind::TaskProgress,
            },
        };
        self.activity_log.push(entry);
    }

    fn advance_to_next_question(&mut self) {
        if self.current_question_index >= self.pending_questions.len() {
            self.mode = AppMode::Running;
        } else {
            let q = self.pending_questions[self.current_question_index].clone();
            if q.multi_select {
                let n = q.options.len() + 1; // +1 for "Other"
                let checked = vec![false; n];
                self.mode = AppMode::MultiSelect {
                    question: q,
                    cursor: 0,
                    checked,
                    other_text: String::new(),
                    typing_other: false,
                };
            } else {
                self.mode = AppMode::Select {
                    question: q,
                    selected: 0,
                    other_text: String::new(),
                    typing_other: false,
                };
            }
        }
    }
}

/// Returns `true` when both stdin and stderr are terminals — indicating the TUI should run.
/// In piped/non-interactive mode (`false`), plain mode is used instead.
pub fn should_run_tui(stdin_is_terminal: bool, stderr_is_terminal: bool) -> bool {
    stdin_is_terminal && stderr_is_terminal
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::event::TuiEvent;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use tddy_core::QuestionOption;

    fn enter_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Enter, KeyModifiers::empty())
    }

    fn char_key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty())
    }

    fn make_question(text: &str, options: &[&str], multi: bool) -> ClarificationQuestion {
        ClarificationQuestion {
            header: "Header".to_string(),
            question: text.to_string(),
            options: options
                .iter()
                .map(|o| QuestionOption {
                    label: o.to_string(),
                    description: String::new(),
                })
                .collect(),
            multi_select: multi,
        }
    }

    /// AC4 + AC1: AppState starts in FeatureInput, transitions through the full workflow lifecycle.
    /// FeatureInput → Running (on Enter) → Select (on ClarificationNeeded) →
    /// Running (on answering last question) → Done (on WorkflowComplete).
    #[test]
    fn test_app_state_transitions() {
        let mut state = AppState::new("claude", "opus");

        // Starts in FeatureInput mode
        assert!(
            matches!(state.mode, AppMode::FeatureInput { .. }),
            "expected FeatureInput mode on init"
        );

        // Type a feature description, then Enter → Running
        state.handle_event(TuiEvent::Key(char_key('t')));
        state.handle_event(TuiEvent::Key(char_key('e')));
        state.handle_event(TuiEvent::Key(char_key('s')));
        state.handle_event(TuiEvent::Key(char_key('t')));
        state.handle_event(TuiEvent::Key(enter_key()));
        assert!(
            matches!(state.mode, AppMode::Running),
            "expected Running after user submits feature description"
        );

        // ClarificationNeeded with one question → Select
        let q = make_question("Which approach?", &["Option A", "Option B"], false);
        state.handle_event(TuiEvent::ClarificationNeeded { questions: vec![q] });
        assert!(
            matches!(state.mode, AppMode::Select { .. }),
            "expected Select mode after ClarificationNeeded"
        );

        // Enter on the first option (only one question) → back to Running
        state.handle_event(TuiEvent::Key(enter_key()));
        assert!(
            matches!(state.mode, AppMode::Running),
            "expected Running after answering the only clarification question"
        );

        // WorkflowComplete → Done
        state.handle_event(TuiEvent::WorkflowComplete(Ok("done".to_string())));
        assert!(
            matches!(state.mode, AppMode::Done),
            "expected Done after WorkflowComplete"
        );
    }

    /// AC3: ProgressEvent variants are appended to the activity log with correct text and kind.
    #[test]
    fn test_activity_log_from_progress_events() {
        let mut state = AppState::new("claude", "opus");
        // Advance to Running mode first
        state.handle_event(TuiEvent::Key(char_key('t')));
        state.handle_event(TuiEvent::Key(enter_key()));

        let before = state.activity_log.len();

        state.handle_event(TuiEvent::Progress(ProgressEvent::ToolUse {
            name: "Read".to_string(),
            detail: Some("src/main.rs".to_string()),
        }));
        state.handle_event(TuiEvent::Progress(ProgressEvent::TaskStarted {
            description: "Planning implementation".to_string(),
        }));
        state.handle_event(TuiEvent::Progress(ProgressEvent::TaskProgress {
            description: "Running tests".to_string(),
            last_tool: Some("Bash".to_string()),
        }));

        assert_eq!(
            state.activity_log.len(),
            before + 3,
            "expected 3 new activity log entries"
        );

        let tool_entry = &state.activity_log[before];
        assert!(
            tool_entry.text.contains("Read"),
            "tool entry should mention tool name 'Read': {}",
            tool_entry.text
        );
        assert!(
            tool_entry.text.contains("src/main.rs"),
            "tool entry should include detail: {}",
            tool_entry.text
        );
        assert_eq!(tool_entry.kind, ActivityKind::ToolUse);

        let task_entry = &state.activity_log[before + 1];
        assert!(
            task_entry.text.contains("Planning implementation"),
            "TaskStarted entry should include description: {}",
            task_entry.text
        );
        assert_eq!(task_entry.kind, ActivityKind::TaskStarted);

        let progress_entry = &state.activity_log[before + 2];
        assert!(
            progress_entry.text.contains("Running tests"),
            "TaskProgress entry should include description: {}",
            progress_entry.text
        );
        assert_eq!(progress_entry.kind, ActivityKind::TaskProgress);
    }

    /// Agent output chunks are coalesced; newlines only when \n is received in a chunk.
    #[test]
    fn test_agent_output_segments_no_line_break_without_newline() {
        let mut state = AppState::new("claude", "opus");
        state.handle_event(TuiEvent::GoalStarted("plan".to_string()));

        // Chunks without newline: coalesce into one line
        state.handle_event(TuiEvent::AgentOutput("Hello ".to_string()));
        state.handle_event(TuiEvent::AgentOutput("world".to_string()));
        assert_eq!(state.activity_log.len(), 0, "no complete line yet");
        assert_eq!(state.agent_output_buffer, "Hello world");

        // Chunk with newline: flush buffer as one line
        state.handle_event(TuiEvent::AgentOutput("\n".to_string()));
        assert_eq!(state.activity_log.len(), 1);
        assert_eq!(state.activity_log[0].text, "Hello world");
        assert!(state.agent_output_buffer.is_empty());

        // Chunk with content and newline: single line
        state.handle_event(TuiEvent::AgentOutput("Next line\n".to_string()));
        assert_eq!(state.activity_log.len(), 2);
        assert_eq!(state.activity_log[1].text, "Next line");
    }

    /// AC8: should_run_tui returns true only when both stdin and stderr are terminals.
    #[test]
    fn test_tty_detection_dispatch() {
        assert!(
            !should_run_tui(false, false),
            "both non-TTY: TUI must not run"
        );
        assert!(
            !should_run_tui(true, false),
            "stderr non-TTY: TUI must not run"
        );
        assert!(
            !should_run_tui(false, true),
            "stdin non-TTY: TUI must not run"
        );
        assert!(should_run_tui(true, true), "both TTY: TUI must run");
    }

    fn up_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Up, KeyModifiers::empty())
    }

    fn down_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Down, KeyModifiers::empty())
    }

    fn esc_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Esc, KeyModifiers::empty())
    }

    // ── Inbox acceptance tests ──────────────────────────────────────────

    /// AC1: Typing in Running mode and pressing Enter adds the text to the inbox queue.
    /// The prompt is NOT sent to the agent (submitted_feature_input stays None).
    #[test]
    fn test_running_mode_input_adds_to_inbox() {
        let mut state = AppState::new("claude", "opus");
        state.mode = AppMode::Running;

        // Type "fix bug" and press Enter
        for c in "fix bug".chars() {
            state.handle_event(TuiEvent::Key(char_key(c)));
        }
        state.handle_event(TuiEvent::Key(enter_key()));

        assert_eq!(
            state.inbox.len(),
            1,
            "inbox must contain 1 item after Enter"
        );
        assert_eq!(
            state.inbox[0], "fix bug",
            "inbox item must match typed text"
        );
        assert!(
            state.submitted_feature_input.is_none(),
            "Running-mode input must NOT set submitted_feature_input"
        );
        assert!(
            matches!(state.mode, AppMode::Running),
            "mode must remain Running after queuing a prompt"
        );
        assert!(
            state.running_input.is_empty(),
            "running_input must be cleared after queuing"
        );
    }

    /// AC4: Up/Down arrows (when input empty) navigate inbox items.
    /// Cursor stays within bounds.
    #[test]
    fn test_inbox_navigation_up_down() {
        let mut state = AppState::new("claude", "opus");
        state.mode = AppMode::Running;
        state.inbox = vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string(),
        ];

        // Press Up on empty running_input to enter inbox list
        state.handle_event(TuiEvent::Key(up_key()));
        assert_eq!(
            state.inbox_focus,
            InboxFocus::List,
            "Up on empty input must switch focus to inbox list"
        );

        // Inbox cursor should start at the last item
        assert_eq!(
            state.inbox_cursor, 2,
            "inbox_cursor must start at last item index"
        );

        // Press Up to move to item index 1
        state.handle_event(TuiEvent::Key(up_key()));
        assert_eq!(state.inbox_cursor, 1, "Up must move cursor up by 1");

        // Press Down to move back to item index 2
        state.handle_event(TuiEvent::Key(down_key()));
        assert_eq!(state.inbox_cursor, 2, "Down must move cursor down by 1");

        // Press Down at last item: cursor stays clamped
        state.handle_event(TuiEvent::Key(down_key()));
        assert_eq!(state.inbox_cursor, 2, "Down at last item must clamp cursor");

        // Press Esc to return to input focus
        state.handle_event(TuiEvent::Key(esc_key()));
        assert_eq!(
            state.inbox_focus,
            InboxFocus::None,
            "Esc must return focus to input"
        );
    }

    /// AC5: Pressing E on a selected inbox item enters edit mode.
    /// Enter saves the edit.
    #[test]
    fn test_inbox_edit_saves_on_enter() {
        let mut state = AppState::new("claude", "opus");
        state.mode = AppMode::Running;
        state.inbox = vec!["original text".to_string()];
        state.inbox_focus = InboxFocus::List;
        state.inbox_cursor = 0;

        // Press E to enter edit mode
        state.handle_event(TuiEvent::Key(char_key('E')));
        assert_eq!(
            state.inbox_focus,
            InboxFocus::Editing,
            "E must switch to Editing focus"
        );
        assert_eq!(
            state.inbox_edit_buffer, "original text",
            "edit buffer must be populated from current inbox item"
        );

        // Clear and type new text (simulate backspacing all then typing)
        // For simplicity, directly set the buffer and simulate Enter
        state.inbox_edit_buffer = "updated text".to_string();
        state.handle_event(TuiEvent::Key(enter_key()));

        assert_eq!(
            state.inbox[0], "updated text",
            "Enter must save edited text back to inbox item"
        );
        assert_eq!(
            state.inbox_focus,
            InboxFocus::List,
            "After saving, focus must return to List"
        );
    }

    /// AC5: Pressing Esc during edit mode discards changes.
    #[test]
    fn test_inbox_edit_discards_on_esc() {
        let mut state = AppState::new("claude", "opus");
        state.mode = AppMode::Running;
        state.inbox = vec!["keep this".to_string()];
        state.inbox_focus = InboxFocus::List;
        state.inbox_cursor = 0;

        // Enter edit mode
        state.handle_event(TuiEvent::Key(char_key('E')));
        assert_eq!(state.inbox_focus, InboxFocus::Editing);

        // Modify the buffer
        state.inbox_edit_buffer = "throw away".to_string();

        // Press Esc to discard
        state.handle_event(TuiEvent::Key(esc_key()));

        assert_eq!(
            state.inbox[0], "keep this",
            "Esc must discard changes — original text preserved"
        );
        assert_eq!(
            state.inbox_focus,
            InboxFocus::List,
            "After Esc, focus must return to List"
        );
    }

    /// AC6: Pressing D on a selected inbox item removes it from the queue.
    #[test]
    fn test_inbox_delete_removes_item() {
        let mut state = AppState::new("claude", "opus");
        state.mode = AppMode::Running;
        state.inbox = vec!["first".to_string(), "second".to_string()];
        state.inbox_focus = InboxFocus::List;
        state.inbox_cursor = 0;

        state.handle_event(TuiEvent::Key(char_key('D')));

        assert_eq!(state.inbox.len(), 1, "D must remove one item from inbox");
        assert_eq!(
            state.inbox[0], "second",
            "remaining item must be 'second' (first was deleted)"
        );

        // Delete the last remaining item
        state.handle_event(TuiEvent::Key(char_key('D')));
        assert!(
            state.inbox.is_empty(),
            "inbox must be empty after deleting all items"
        );
        assert_eq!(
            state.inbox_focus,
            InboxFocus::None,
            "focus must return to input when inbox becomes empty"
        );
    }

    /// AC7: On WorkflowComplete(Ok) with non-empty inbox, the first item is dequeued
    /// and set as submitted_feature_input for dispatch.
    #[test]
    fn test_workflow_complete_dequeues_inbox() {
        let mut state = AppState::new("claude", "opus");
        state.mode = AppMode::Running;
        state.inbox = vec!["queued task 1".to_string(), "queued task 2".to_string()];

        state.handle_event(TuiEvent::WorkflowComplete(Ok("done".to_string())));

        assert_eq!(state.inbox.len(), 1, "one item must be dequeued from inbox");
        assert_eq!(
            state.inbox[0], "queued task 2",
            "remaining item must be the second one"
        );
        assert!(
            state.submitted_feature_input.is_some(),
            "dequeued prompt must be set as submitted_feature_input"
        );
        assert!(
            matches!(state.mode, AppMode::Running),
            "mode must remain Running when inbox had items"
        );
    }

    /// AC8: On WorkflowComplete(Ok) with empty inbox, TUI transitions to Done.
    /// Also verifies the inbox branch: first complete dequeues, second complete (empty inbox) goes to Done.
    #[test]
    fn test_workflow_complete_done_when_empty() {
        let mut state = AppState::new("claude", "opus");
        state.mode = AppMode::Running;
        state.inbox = vec!["only task".to_string()];

        // First WorkflowComplete: inbox non-empty → dequeue, stay Running
        state.handle_event(TuiEvent::WorkflowComplete(Ok("cycle 1 done".to_string())));
        assert!(
            matches!(state.mode, AppMode::Running),
            "mode must stay Running when inbox had items"
        );
        assert!(
            state.submitted_feature_input.is_some(),
            "dequeued prompt must be set"
        );
        state.submitted_feature_input = None; // simulate dispatch

        // Second WorkflowComplete: inbox is now empty → Done
        state.handle_event(TuiEvent::WorkflowComplete(Ok("cycle 2 done".to_string())));
        assert!(
            matches!(state.mode, AppMode::Done),
            "mode must transition to Done when inbox is empty"
        );
        assert!(
            state.submitted_feature_input.is_none(),
            "submitted_feature_input must remain None when inbox is empty"
        );
    }

    /// AC10: The dequeued prompt contains an instruction prefix telling the agent
    /// that the items were queued.
    #[test]
    fn test_dequeued_prompt_has_instruction_prefix() {
        let mut state = AppState::new("claude", "opus");
        state.mode = AppMode::Running;
        state.inbox = vec!["fix the login bug".to_string()];

        state.handle_event(TuiEvent::WorkflowComplete(Ok("done".to_string())));

        let prompt = state
            .submitted_feature_input
            .as_ref()
            .expect("dequeued prompt must be set");
        assert!(
            prompt.contains("fix the login bug"),
            "dequeued prompt must contain the original queued text"
        );
        assert!(
            prompt.len() > "fix the login bug".len(),
            "dequeued prompt must be longer than the raw text (has instruction prefix)"
        );
        let lower = prompt.to_lowercase();
        assert!(
            lower.contains("queued") || lower.contains("queue"),
            "instruction prefix must mention 'queued': {}",
            prompt
        );
    }

    // ── Lower-level / granular inbox tests ────────────────────────────

    /// Running mode: typing characters appends to running_input.
    #[test]
    fn test_running_mode_char_appends_to_running_input() {
        let mut state = AppState::new("claude", "opus");
        state.mode = AppMode::Running;

        state.handle_event(TuiEvent::Key(char_key('a')));
        state.handle_event(TuiEvent::Key(char_key('b')));

        assert_eq!(
            state.running_input, "ab",
            "chars typed in Running mode must append to running_input"
        );
        assert_eq!(
            state.running_cursor, 2,
            "running_cursor must advance with each char"
        );
    }

    /// Running mode: Backspace removes last char from running_input.
    #[test]
    fn test_running_mode_backspace_removes_char() {
        let mut state = AppState::new("claude", "opus");
        state.mode = AppMode::Running;
        state.running_input = "abc".to_string();
        state.running_cursor = 3;

        let backspace = KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty());
        state.handle_event(TuiEvent::Key(backspace));

        assert_eq!(
            state.running_input, "ab",
            "Backspace must remove last char from running_input"
        );
        assert_eq!(state.running_cursor, 2, "cursor must decrement");
    }

    /// Running mode: multiple Enter presses queue multiple items.
    #[test]
    fn test_running_mode_multiple_items_queued() {
        let mut state = AppState::new("claude", "opus");
        state.mode = AppMode::Running;

        for c in "task one".chars() {
            state.handle_event(TuiEvent::Key(char_key(c)));
        }
        state.handle_event(TuiEvent::Key(enter_key()));

        for c in "task two".chars() {
            state.handle_event(TuiEvent::Key(char_key(c)));
        }
        state.handle_event(TuiEvent::Key(enter_key()));

        assert_eq!(state.inbox.len(), 2, "two items must be queued");
        assert_eq!(state.inbox[0], "task one");
        assert_eq!(state.inbox[1], "task two");
    }

    /// Delete last item at end of list: cursor adjusts to previous item.
    #[test]
    fn test_inbox_delete_adjusts_cursor_at_end() {
        let mut state = AppState::new("claude", "opus");
        state.mode = AppMode::Running;
        state.inbox = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        state.inbox_focus = InboxFocus::List;
        state.inbox_cursor = 2; // pointing at last item "c"

        state.handle_event(TuiEvent::Key(char_key('D')));

        assert_eq!(state.inbox.len(), 2, "one item removed");
        assert_eq!(
            state.inbox_cursor, 1,
            "cursor must adjust to last remaining item"
        );
    }

    /// AC5 + AC7 (flow): Multiple pending questions are answered sequentially;
    /// all answers are collected and joined with '\n'.
    #[test]
    fn test_clarification_roundtrip_collects_all_answers() {
        let mut state = AppState::new("claude", "opus");
        // Advance to Running
        state.handle_event(TuiEvent::Key(char_key('t')));
        state.handle_event(TuiEvent::Key(enter_key()));

        let questions = vec![
            make_question("First question?", &["Alpha", "Beta"], false),
            make_question("Second question?", &["Gamma", "Delta"], false),
        ];
        state.handle_event(TuiEvent::ClarificationNeeded {
            questions: questions.clone(),
        });

        // First question active
        assert!(
            matches!(state.mode, AppMode::Select { .. }),
            "expected Select for first question"
        );
        assert_eq!(state.current_question_index, 0);

        // Answer first question (Enter on default-selected first option = "Alpha")
        state.handle_event(TuiEvent::Key(enter_key()));
        assert_eq!(
            state.current_question_index, 1,
            "after first answer, question index should advance to 1"
        );
        assert!(
            matches!(state.mode, AppMode::Select { .. }),
            "should still be in Select mode for second question"
        );

        // Answer second question (Enter on first option = "Gamma")
        state.handle_event(TuiEvent::Key(enter_key()));
        assert!(
            matches!(state.mode, AppMode::Running),
            "after all questions answered, should return to Running"
        );

        let combined = state.collect_answers();
        assert_eq!(
            combined, "Alpha\nGamma",
            "answers must be joined with newline in order"
        );
    }
}
