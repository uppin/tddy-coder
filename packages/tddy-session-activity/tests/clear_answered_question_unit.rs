//! `clear_answered_pending_question`: once a request is answered, its pending question is cleared —
//! and only it (`#home-inbox` 11/11). The hook has already returned, so the child may have recorded
//! its next question by the time the clear runs; that one is not the answer's to clear.
//!
//! The end-to-end answer, whose tail this is, is `permission_answer_acceptance.rs`.

use std::path::Path;

use pretty_assertions::assert_eq;
use tddy_core::pending_question::{read_pending_question, write_pending_question, PendingQuestion};
use tddy_session_activity::answers::clear_answered_pending_question;

const ANSWERED: &str = "req-cargo-publish";

fn a_permission_question(request_id: &str) -> PendingQuestion {
    PendingQuestion {
        kind: "permission".to_string(),
        question: format!("Run Bash for {request_id}"),
        request_id: request_id.to_string(),
        tool_name: "Bash".to_string(),
        ..Default::default()
    }
}

fn a_session_waiting_on(question: &PendingQuestion) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a session dir");
    write_pending_question(dir.path(), question).expect("the pending question");
    dir
}

fn pending_in(dir: &Path) -> Option<PendingQuestion> {
    read_pending_question(dir).expect("the pending question file is readable")
}

#[test]
fn the_question_that_was_answered_is_cleared() {
    // Given a session still waiting on the question that was answered
    let session = a_session_waiting_on(&a_permission_question(ANSWERED));

    // When the answer clears it
    clear_answered_pending_question(session.path(), ANSWERED);

    // Then the session waits on nothing
    assert_eq!(pending_in(session.path()), None);
}

#[test]
fn a_question_with_another_request_id_is_kept() {
    // Given a session that has already recorded its next question
    let next = a_permission_question("req-git-push");
    let session = a_session_waiting_on(&next);

    // When the answer to the earlier one clears what it answered
    clear_answered_pending_question(session.path(), ANSWERED);

    // Then the next question is still pending, untouched
    assert_eq!(pending_in(session.path()), Some(next));
}

#[test]
fn a_question_with_no_request_id_is_kept() {
    // Given a session waiting on a clarification that carries no request_id
    let clarification = PendingQuestion {
        kind: "select".to_string(),
        question: "Which language?".to_string(),
        ..Default::default()
    };
    let session = a_session_waiting_on(&clarification);

    // When an answer to a request clears what it answered
    clear_answered_pending_question(session.path(), ANSWERED);

    // Then the clarification is still pending: it is not that request
    assert_eq!(pending_in(session.path()), Some(clarification));
}

#[test]
fn a_session_with_no_pending_question_is_left_as_it_is() {
    // Given a session waiting on nothing
    let session = tempfile::tempdir().expect("a session dir");

    // When an answer clears what it answered
    clear_answered_pending_question(session.path(), ANSWERED);

    // Then there is still nothing pending, and no file was made
    assert_eq!(pending_in(session.path()), None);
    assert_eq!(
        std::fs::read_dir(session.path())
            .expect("the session dir")
            .count(),
        0,
        "files in the session dir"
    );
}
