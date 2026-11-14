//! The orchestrator's mark on a session in `.session.yaml`: how it is stored, and that a resume
//! takes a `paused` mark off and leaves every other alone (`#orchestrators` 5/11).
//!
//! Changeset: docs/dev/1-WIP/2026-11-10-session-lifecycle-tools.md

use pretty_assertions::assert_eq;
use tddy_changeset::{
    clear_a_paused_mark, read_session_metadata, write_session_metadata, OrchestrationMark,
    OrchestrationMarkKind, SessionMetadata,
};

fn a_session() -> SessionMetadata {
    serde_yaml::from_str(
        r#"session_id: 018f1234-5678-7abc-8def-123456789abc
project_id: proj-a
created_at: "2026-11-10T12:00:00Z"
updated_at: "2026-11-10T12:00:00Z"
status: active
session_type: workspace
"#,
    )
    .expect("the fixture is a .session.yaml SessionMetadata accepts")
}

fn marked(kind: OrchestrationMarkKind) -> SessionMetadata {
    SessionMetadata {
        orchestration_mark: Some(OrchestrationMark {
            mark: kind,
            note: Some("waiting on #881".to_string()),
            set_by_session_id: "orch-1".to_string(),
            at_unix_ms: 1_794_000_000_000,
        }),
        ..a_session()
    }
}

#[test]
fn a_mark_round_trips_through_session_yaml_under_its_lowercase_word() {
    // Given a session marked blocked
    let dir = tempfile::tempdir().unwrap();
    let session = marked(OrchestrationMarkKind::Blocked);

    // When it is written and read back
    write_session_metadata(dir.path(), &session).unwrap();
    let yaml = std::fs::read_to_string(dir.path().join(".session.yaml")).unwrap();
    let read = read_session_metadata(dir.path()).unwrap();

    // Then the file spells the mark `blocked`, and the mark reads back whole
    assert!(yaml.contains("mark: blocked"), "{yaml}");
    assert_eq!(read.orchestration_mark, session.orchestration_mark);
}

#[test]
fn an_unmarked_session_writes_no_mark_key() {
    // Given an unmarked session
    let dir = tempfile::tempdir().unwrap();

    // When it is written
    write_session_metadata(dir.path(), &a_session()).unwrap();

    // Then its file reads exactly as one written before marks existed
    let yaml = std::fs::read_to_string(dir.path().join(".session.yaml")).unwrap();
    assert!(!yaml.contains("orchestration_mark"), "{yaml}");
}

#[test]
fn a_resume_takes_a_paused_mark_off() {
    // Given a session marked paused
    let mut session = marked(OrchestrationMarkKind::Paused);

    // When a resume clears it
    let cleared = clear_a_paused_mark(&mut session);

    // Then the mark is gone, and that is reported
    assert!(cleared, "a paused mark was not reported cleared");
    assert_eq!(session.orchestration_mark, None);
}

#[test]
fn a_resume_leaves_a_blocked_or_done_mark_alone() {
    // Given sessions marked blocked and done
    let mut blocked = marked(OrchestrationMarkKind::Blocked);
    let mut done = marked(OrchestrationMarkKind::Done);

    // When a resume clears each
    let cleared = (
        clear_a_paused_mark(&mut blocked),
        clear_a_paused_mark(&mut done),
    );

    // Then neither changed
    assert_eq!(cleared, (false, false));
    assert_eq!(
        blocked.orchestration_mark,
        marked(OrchestrationMarkKind::Blocked).orchestration_mark
    );
    assert_eq!(
        done.orchestration_mark,
        marked(OrchestrationMarkKind::Done).orchestration_mark
    );
}
