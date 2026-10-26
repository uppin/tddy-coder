//! A `tool` session's workflow question is recorded where an orchestrator can read it, and cleared
//! when the workflow moves on (`#start-parity` 6/9).

use pretty_assertions::assert_eq;
use tddy_coder::session_participant::pending_question::{
    pending_question_for_mode, record_pending_question, spawn_pending_question_recorder,
};
use tddy_core::pending_question::{read_pending_question, PendingQuestion};
use tddy_core::{
    AppMode, ClarificationQuestion, ModeChangedDetails, PresenterEvent, QuestionOption,
};

fn a_clarification(multi_select: bool) -> ClarificationQuestion {
    ClarificationQuestion {
        header: "Scope".to_string(),
        question: "Which packages does this touch?".to_string(),
        options: vec![
            QuestionOption {
                label: "core".to_string(),
                description: String::new(),
            },
            QuestionOption {
                label: "web".to_string(),
                description: String::new(),
            },
        ],
        multi_select,
        allow_other: false,
    }
}

#[test]
fn a_select_mode_is_a_select_question_with_its_options() {
    // Given
    let mode = AppMode::Select {
        question: a_clarification(false),
        question_index: 0,
        total_questions: 1,
        initial_selected: 0,
    };

    // Then
    assert_eq!(
        pending_question_for_mode(&mode),
        Some(PendingQuestion {
            kind: "select".to_string(),
            header: "Scope".to_string(),
            question: "Which packages does this touch?".to_string(),
            options: vec!["core".to_string(), "web".to_string()],
            allow_other: false,
        })
    );
}

#[test]
fn a_multi_select_and_a_text_prompt_are_questions_too() {
    // Given
    let multi = AppMode::MultiSelect {
        question: a_clarification(true),
        question_index: 0,
        total_questions: 1,
    };
    let text = AppMode::TextInput {
        prompt: "Name the branch".to_string(),
    };

    // Then
    assert_eq!(
        pending_question_for_mode(&multi).unwrap().kind,
        "multi_select"
    );
    let text_question = pending_question_for_mode(&text).unwrap();
    assert_eq!(
        (text_question.kind.as_str(), text_question.question.as_str()),
        ("text", "Name the branch")
    );
}

#[test]
fn a_running_workflow_waits_on_nothing() {
    assert_eq!(pending_question_for_mode(&AppMode::Running), None);
}

#[test]
fn the_question_is_recorded_while_asked_and_cleared_after() {
    // Given
    let session_dir = tempfile::tempdir().unwrap();
    let asking = AppMode::Select {
        question: a_clarification(false),
        question_index: 0,
        total_questions: 1,
        initial_selected: 0,
    };

    // When asked
    record_pending_question(session_dir.path(), &asking).unwrap();

    // Then it is recorded
    assert_eq!(
        read_pending_question(session_dir.path())
            .unwrap()
            .map(|q| q.question),
        Some("Which packages does this touch?".to_string())
    );

    // And when the workflow runs on
    record_pending_question(session_dir.path(), &AppMode::Running).unwrap();

    // Then it is cleared
    assert_eq!(read_pending_question(session_dir.path()).unwrap(), None);
}

fn mode_changed(mode: AppMode) -> PresenterEvent {
    PresenterEvent::ModeChanged(ModeChangedDetails {
        mode,
        plan_refinement_pending: false,
        skills_project_root: None,
        awaiting_open_answer: false,
    })
}

#[tokio::test]
async fn the_recorder_follows_the_presenters_mode_until_its_events_end() {
    // Given a recorder on a session's presenter events
    let session_dir = tempfile::tempdir().unwrap();
    let (events, rx) = tokio::sync::broadcast::channel(8);
    let recorder = spawn_pending_question_recorder(rx, session_dir.path().to_path_buf());

    // When the workflow asks, then runs on, and the presenter goes away
    events
        .send(mode_changed(AppMode::TextInput {
            prompt: "Name the branch".to_string(),
        }))
        .unwrap();
    events.send(mode_changed(AppMode::Running)).unwrap();
    drop(events);
    recorder.await.unwrap();

    // Then the question was cleared once the workflow ran on, and the recorder stopped
    assert_eq!(read_pending_question(session_dir.path()).unwrap(), None);
}

#[tokio::test]
async fn the_recorder_keeps_the_question_while_it_is_asked() {
    // Given
    let session_dir = tempfile::tempdir().unwrap();
    let (events, rx) = tokio::sync::broadcast::channel(8);
    let recorder = spawn_pending_question_recorder(rx, session_dir.path().to_path_buf());

    // When
    events
        .send(mode_changed(AppMode::TextInput {
            prompt: "Name the branch".to_string(),
        }))
        .unwrap();
    drop(events);
    recorder.await.unwrap();

    // Then
    assert_eq!(
        read_pending_question(session_dir.path())
            .unwrap()
            .map(|q| q.question),
        Some("Name the branch".to_string())
    );
}
