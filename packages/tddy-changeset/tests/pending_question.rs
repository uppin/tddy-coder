//! The pending-question file: written while a session waits, read by whoever follows it, cleared
//! when it stops waiting (`#start-parity` 6/9).

use pretty_assertions::assert_eq;
use tddy_changeset::pending_question::{
    clear_pending_question, read_pending_question, write_pending_question, PendingQuestion,
    PENDING_QUESTION_FILE,
};

fn a_question() -> PendingQuestion {
    PendingQuestion {
        kind: "select".to_string(),
        header: "Scope".to_string(),
        question: "Which packages does this touch?".to_string(),
        options: vec!["core".to_string(), "web".to_string()],
        allow_other: true,
        ..Default::default()
    }
}

#[test]
fn a_written_question_is_read_back_whole() {
    // Given
    let dir = tempfile::tempdir().unwrap();

    // When
    write_pending_question(dir.path(), &a_question()).unwrap();

    // Then
    assert_eq!(
        read_pending_question(dir.path()).unwrap(),
        Some(a_question())
    );
    assert!(dir.path().join(PENDING_QUESTION_FILE).is_file());
}

#[test]
fn a_new_question_replaces_the_last() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    write_pending_question(dir.path(), &a_question()).unwrap();
    let next = PendingQuestion {
        kind: "text".to_string(),
        question: "Name the branch".to_string(),
        ..Default::default()
    };

    // When
    write_pending_question(dir.path(), &next).unwrap();

    // Then
    assert_eq!(read_pending_question(dir.path()).unwrap(), Some(next));
}

#[test]
fn a_cleared_session_waits_on_nothing_and_clearing_twice_is_fine() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    write_pending_question(dir.path(), &a_question()).unwrap();

    // When
    clear_pending_question(dir.path()).unwrap();
    clear_pending_question(dir.path()).unwrap();

    // Then
    assert_eq!(read_pending_question(dir.path()).unwrap(), None);
}

#[test]
fn a_session_that_never_waited_has_no_question() {
    // Given
    let dir = tempfile::tempdir().unwrap();

    // Then
    assert_eq!(read_pending_question(dir.path()).unwrap(), None);
}
