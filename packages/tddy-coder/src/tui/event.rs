//! TUI event types passed from workflow thread and crossterm event reader to the main event loop.

use tddy_core::{ClarificationQuestion, ProgressEvent};

/// All events the main TUI event loop processes.
#[derive(Debug)]
pub enum TuiEvent {
    /// Keyboard input from the user.
    Key(crossterm::event::KeyEvent),
    /// Terminal was resized.
    Resize(u16, u16),
    /// Progress event from the workflow thread (tool use, task progress, etc.).
    Progress(ProgressEvent),
    /// Workflow state changed.
    StateChange { from: String, to: String },
    /// A new workflow goal started.
    GoalStarted(String),
    /// The workflow needs clarification before continuing.
    ClarificationNeeded {
        questions: Vec<ClarificationQuestion>,
    },
    /// The workflow thread completed (Ok = summary message, Err = error).
    WorkflowComplete(Result<String, String>),
}
