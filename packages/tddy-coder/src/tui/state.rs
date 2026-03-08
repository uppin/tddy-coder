//! Application state for the TUI: modes, activity log, status tracking.

use std::time::Instant;
use tddy_core::{ClarificationQuestion, ProgressEvent};

use crate::tui::event::TuiEvent;
use crate::tui::input::{handle_multiselect_key, handle_select_key, handle_text_input_key};

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
}

/// A single entry in the scrollable activity log.
#[derive(Debug, Clone)]
pub struct ActivityEntry {
    pub text: String,
    pub kind: ActivityKind,
}

/// Top-level application state for the TUI event loop.
pub struct AppState {
    pub mode: AppMode,
    pub current_goal: Option<String>,
    pub current_state: Option<String>,
    pub goal_start_time: Instant,
    pub activity_log: Vec<ActivityEntry>,
    pub auto_scroll: bool,
    pub should_quit: bool,
    /// All questions in the current clarification round.
    pub pending_questions: Vec<ClarificationQuestion>,
    /// Index into `pending_questions` of the question currently being shown.
    pub current_question_index: usize,
    /// Answers collected so far in this clarification round (one per question answered).
    pub collected_answers: Vec<String>,
    /// When user submits feature input (Enter in FeatureInput mode), stored here for the event loop to send.
    pub submitted_feature_input: Option<String>,
}

impl AppState {
    /// Create a new AppState in FeatureInput mode.
    pub fn new() -> Self {
        AppState {
            mode: AppMode::FeatureInput {
                input: String::new(),
                cursor: 0,
            },
            current_goal: None,
            current_state: None,
            goal_start_time: Instant::now(),
            activity_log: Vec::new(),
            auto_scroll: true,
            should_quit: false,
            pending_questions: Vec::new(),
            current_question_index: 0,
            collected_answers: Vec::new(),
            submitted_feature_input: None,
        }
    }

    /// Process a TuiEvent and update state accordingly.
    pub fn handle_event(&mut self, event: TuiEvent) {
        match event {
            TuiEvent::Key(key) => self.handle_key(key),
            TuiEvent::Progress(ev) => self.handle_progress(ev),
            TuiEvent::ClarificationNeeded { questions } => {
                self.pending_questions = questions;
                self.current_question_index = 0;
                self.collected_answers.clear();
                self.advance_to_next_question();
            }
            TuiEvent::WorkflowComplete(_) => {
                self.mode = AppMode::Done;
            }
            TuiEvent::GoalStarted(goal) => {
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
            TuiEvent::Resize(_, _) => {}
        }
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
                self.mode = AppMode::Running;
            }
            AppMode::Done => {
                self.mode = AppMode::Done;
            }
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
        let mut state = AppState::new();

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
        let mut state = AppState::new();
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

    /// AC5 + AC7 (flow): Multiple pending questions are answered sequentially;
    /// all answers are collected and joined with '\n'.
    #[test]
    fn test_clarification_roundtrip_collects_all_answers() {
        let mut state = AppState::new();
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
