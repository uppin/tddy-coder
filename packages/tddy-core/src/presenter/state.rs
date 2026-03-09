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

/// The current interaction mode (minimal — no input buffers).
#[derive(Debug, Clone)]
pub enum AppMode {
    /// Waiting for the user to type a feature description.
    FeatureInput,
    /// Workflow is running.
    Running,
    /// Presenting a single-select clarification question.
    Select {
        question: ClarificationQuestion,
        question_index: usize,
        total_questions: usize,
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
}

/// Top-level application state owned by the Presenter.
#[derive(Debug)]
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
    fn activity_entry_has_text_and_kind() {
        let entry = ActivityEntry {
            text: "Tool: Read".to_string(),
            kind: ActivityKind::ToolUse,
        };
        assert_eq!(entry.text, "Tool: Read");
        assert_eq!(entry.kind, ActivityKind::ToolUse);
    }
}
