//! A `tool` session records the clarification its workflow is waiting on in its session directory,
//! so an orchestrator following it with `ReadSessionActivity` sees the question, not only that one
//! is open (`#start-parity` 6/9).
//!
//! The presenter's mode is the source: a select, multi-select or free-text mode is a pending
//! question, and any other mode means the session is not waiting.

use std::io;
use std::path::{Path, PathBuf};

use tddy_core::pending_question::{
    clear_pending_question, write_pending_question, PendingQuestion,
};
use tddy_core::{AppMode, ClarificationQuestion, PresenterEvent};

/// The question `mode` is waiting on, or `None` when it waits on nothing.
pub fn pending_question_for_mode(mode: &AppMode) -> Option<PendingQuestion> {
    match mode {
        AppMode::Select { question, .. } => Some(clarification_question("select", question)),
        AppMode::MultiSelect { question, .. } => {
            Some(clarification_question("multi_select", question))
        }
        AppMode::TextInput { prompt } => Some(PendingQuestion {
            kind: "text".to_string(),
            question: prompt.clone(),
            ..Default::default()
        }),
        _ => None,
    }
}

fn clarification_question(kind: &str, question: &ClarificationQuestion) -> PendingQuestion {
    PendingQuestion {
        kind: kind.to_string(),
        header: question.header.clone(),
        question: question.question.clone(),
        options: question.options.iter().map(|o| o.label.clone()).collect(),
        allow_other: question.allow_other,
        ..Default::default()
    }
}

/// Record (or clear) `session_dir`'s pending question for `mode`.
pub fn record_pending_question(session_dir: &Path, mode: &AppMode) -> io::Result<()> {
    match pending_question_for_mode(mode) {
        Some(question) => write_pending_question(session_dir, &question),
        None => clear_pending_question(session_dir),
    }
}

/// Spawn a task that keeps `session_dir`'s pending question in step with the presenter's mode: on
/// every `ModeChanged` the question the new mode asks is recorded, or the last one cleared. The task
/// ends when the presenter's event channel closes.
pub fn spawn_pending_question_recorder(
    mut event_rx: tokio::sync::broadcast::Receiver<PresenterEvent>,
    session_dir: PathBuf,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match event_rx.recv().await {
                Ok(PresenterEvent::ModeChanged(details)) => {
                    if let Err(e) = record_pending_question(&session_dir, &details.mode) {
                        log::warn!(
                            target: "tddy_coder::pending_question",
                            "could not record the pending question in {}: {e}",
                            session_dir.display()
                        );
                    }
                }
                // A lagged receiver skipped events it cannot recover; the last recorded question
                // stands until the next `ModeChanged` replaces or clears it.
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}
