//! Application state owned by the Presenter.

use crate::ClarificationQuestion;
use std::time::Instant;

/// Kind of entry in the activity log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityKind {
    ToolUse,
    TaskStarted,
    TaskProgress,
    StateChange,
    Info,
    AgentOutput,
}

/// A single entry in the scrollable activity log.
#[derive(Debug, Clone)]
pub struct ActivityEntry {
    pub text: String,
    pub kind: ActivityKind,
}

/// Action to perform after TUI exits (e.g. exec into claude terminal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitAction {
    /// User chose "Continue with agent" — exec into `claude --resume <session_id>`.
    ContinueWithAgent { session_id: String },
}

/// The current interaction mode (minimal — no input buffers).
#[derive(Debug, Clone)]
pub enum AppMode {
    /// Waiting for the user to type a feature description.
    FeatureInput,
    /// Workflow is running.
    Running,
    /// Plan approval gate: View, Approve, or Refine.
    PlanReview { prd_content: String },
    /// Full-screen markdown viewer (PRD content).
    MarkdownViewer { content: String },
    /// Presenting a single-select clarification question.
    Select {
        question: ClarificationQuestion,
        question_index: usize,
        total_questions: usize,
        /// Initial highlighted option index (0-based).
        initial_selected: usize,
    },
    /// Presenting a multi-select clarification question.
    MultiSelect {
        question: ClarificationQuestion,
        question_index: usize,
        total_questions: usize,
    },
    /// Free-form text input (question with no predefined options).
    TextInput { prompt: String },
    /// Workflow complete.
    Done,
    /// Workflow errored — user can Resume or Exit.
    ErrorRecovery { error_message: String },
}

/// Top-level application state owned by the Presenter.
#[derive(Debug, Clone)]
pub struct PresenterState {
    pub agent: String,
    pub model: String,
    pub mode: AppMode,
    pub current_goal: Option<String>,
    pub current_state: Option<String>,
    pub goal_start_time: Instant,
    pub activity_log: Vec<ActivityEntry>,
    pub inbox: Vec<String>,
    pub should_quit: bool,
    /// When set, the TUI caller should perform this action after exit.
    pub exit_action: Option<ExitAction>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_mode_feature_input() {
        let mode = AppMode::FeatureInput;
        assert!(matches!(mode, AppMode::FeatureInput));
    }

    #[test]
    fn app_mode_running() {
        let mode = AppMode::Running;
        assert!(matches!(mode, AppMode::Running));
    }

    #[test]
    fn app_mode_done() {
        let mode = AppMode::Done;
        assert!(matches!(mode, AppMode::Done));
    }

    #[test]
    fn test_exit_action_continue_with_agent() {
        let action = ExitAction::ContinueWithAgent {
            session_id: "abc-123".to_string(),
        };
        match action {
            ExitAction::ContinueWithAgent { ref session_id } => {
                assert_eq!(session_id, "abc-123");
            }
        }
    }

    #[test]
    fn test_presenter_state_exit_action_default_none() {
        let state = PresenterState {
            agent: "test".to_string(),
            model: "test".to_string(),
            mode: AppMode::FeatureInput,
            current_goal: None,
            current_state: None,
            goal_start_time: std::time::Instant::now(),
            activity_log: Vec::new(),
            inbox: Vec::new(),
            should_quit: false,
            exit_action: None,
        };
        assert!(state.exit_action.is_none());
    }

    #[test]
    fn test_app_mode_error_recovery_construction() {
        let mode = AppMode::ErrorRecovery {
            error_message: "backend timeout".to_string(),
        };
        match mode {
            AppMode::ErrorRecovery { ref error_message } => {
                assert_eq!(error_message, "backend timeout");
            }
            _ => panic!("Expected ErrorRecovery variant"),
        }
    }

    #[test]
    fn activity_entry_has_text_and_kind() {
        let entry = ActivityEntry {
            text: "Tool: Read".to_string(),
            kind: ActivityKind::ToolUse,
        };
        assert_eq!(entry.text, "Tool: Read");
        assert_eq!(entry.kind, ActivityKind::ToolUse);
    }

    #[test]
    fn app_mode_select_has_initial_selected() {
        let mode = AppMode::Select {
            question: ClarificationQuestion {
                header: "test".to_string(),
                question: "pick one".to_string(),
                options: vec![],
                multi_select: false,
                allow_other: false,
            },
            question_index: 0,
            total_questions: 1,
            initial_selected: 2,
        };
        if let AppMode::Select {
            initial_selected, ..
        } = mode
        {
            assert_eq!(initial_selected, 2);
        } else {
            panic!("expected Select");
        }
    }
}
