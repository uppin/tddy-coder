//! Unit tests: `.session.yaml` persistence for a **split** session.
//!
//! A split session's placement cannot be derived at read time. `SessionEntry.daemon_instance_id` is
//! stamped by whichever daemon answered `ListSessions`, which works only while a session has one
//! host — a split session has two, and each daemon would legitimately claim its own half. So the
//! pairing is persisted (docs/ft/daemon/remote-managed-worktree.md).
//!
//! `SessionMetadata` uses `deny_unknown_fields`, so every new field is a breaking read for older
//! daemons and an absent field must stay absent on the wire rather than serialize as null.

use tddy_core::SessionMetadata;

fn a_session_yaml_with_codebase_placement() -> String {
    r#"
session_id: "019d105b-ac0f-78d3-9a89-409731145a42"
project_id: "proj-1"
created_at: "2026-08-13T12:00:00Z"
updated_at: "2026-08-13T12:00:00Z"
status: "active"
pending_elicitation: false
session_type: "claude-cli"
codebase_daemon_instance_id: "workstation-b"
codebase_session_id: "019d105b-ac0f-78d3-9a89-409731145a43"
"#
    .to_string()
}

/// A pre-existing session file, written before split placement existed.
fn a_legacy_session_yaml() -> String {
    r#"
session_id: "019d105b-ac0f-78d3-9a89-409731145a44"
project_id: "proj-1"
created_at: "2026-08-13T12:00:00Z"
updated_at: "2026-08-13T12:00:00Z"
status: "active"
pending_elicitation: false
session_type: "claude-cli"
repo_path: "/home/dev/repo/.worktrees/feature"
"#
    .to_string()
}

fn a_split_metadata() -> SessionMetadata {
    serde_yaml::from_str(&a_session_yaml_with_codebase_placement())
        .expect("a split session file must parse")
}

#[test]
fn a_split_session_file_records_which_daemon_holds_the_codebase() {
    // When
    let metadata = a_split_metadata();

    // Then
    assert_eq!(
        metadata.codebase_daemon_instance_id.as_deref(),
        Some("workstation-b")
    );
    assert_eq!(
        metadata.codebase_session_id.as_deref(),
        Some("019d105b-ac0f-78d3-9a89-409731145a43")
    );
}

#[test]
fn a_split_session_file_records_no_local_repository() {
    // When
    let metadata = a_split_metadata();

    // Then — there is no checkout on the agent's host at all, so anything resolving a worktree from
    // `repo_path` must treat this as "elsewhere" rather than as a malformed session
    assert_eq!(metadata.repo_path, None);
}

#[test]
fn a_split_session_round_trips_its_placement_through_yaml() {
    // Given
    let original = a_split_metadata();

    // When
    let written = serde_yaml::to_string(&original).expect("serialize");
    let reread: SessionMetadata = serde_yaml::from_str(&written).expect("re-parse");

    // Then
    assert_eq!(
        reread.codebase_daemon_instance_id,
        original.codebase_daemon_instance_id
    );
    assert_eq!(reread.codebase_session_id, original.codebase_session_id);
}

#[test]
fn a_co_located_session_omits_both_codebase_fields_from_the_written_yaml() {
    // Given a session with no split placement
    let metadata: SessionMetadata =
        serde_yaml::from_str(&a_legacy_session_yaml()).expect("legacy file must parse");

    // When
    let written = serde_yaml::to_string(&metadata).expect("serialize");

    // Then — `deny_unknown_fields` means an emitted null would break every reader that predates the
    // field, so absence must be absence
    assert!(
        !written.contains("codebase_daemon_instance_id"),
        "a co-located session must not write the field at all; got:\n{written}"
    );
    assert!(
        !written.contains("codebase_session_id"),
        "a co-located session must not write the field at all; got:\n{written}"
    );
}

#[test]
fn a_session_file_written_before_split_placement_still_parses() {
    // When
    let metadata: SessionMetadata =
        serde_yaml::from_str(&a_legacy_session_yaml()).expect("legacy file must parse");

    // Then — every session on disk today predates these fields
    assert_eq!(metadata.codebase_daemon_instance_id, None);
    assert_eq!(metadata.codebase_session_id, None);
    assert_eq!(
        metadata.repo_path.as_deref(),
        Some("/home/dev/repo/.worktrees/feature")
    );
}
