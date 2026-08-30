//! Unit: resolving a session's label on the daemon side, from the same values `ListSessions`
//! reports to the drawer.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-08-29-session-notifications-as-indicators.md (FR1).
//!
//! `tddy_core::session_display_label` is the rule; this is the lookup that feeds it. Parity is only
//! real if the daemon reads the *same three values the browser receives*: `repo_path` from
//! `.session.yaml`, `workflow_goal` from the session-list enrichment, and the session id. A
//! resolver that read, say, the worktree from `changeset.yaml` instead would name sessions
//! correctly right up until the two sources disagreed.

use std::path::Path;

use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_daemon::session_notifications::resolve_session_label;

const SESSION_ID: &str = "01900000-0000-7000-8000-AABB00000001";

/// A valid claude-cli session's metadata. Cases override only the field they are about.
fn a_session_metadata() -> SessionMetadata {
    SessionMetadata {
        session_id: SESSION_ID.to_string(),
        project_id: "test-project".to_string(),
        created_at: "2026-08-29T10:00:00Z".to_string(),
        updated_at: "2026-08-29T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: None,
        pid: Some(12345),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: Some("claude-opus-5".to_string()),
        cursor_chat_id: None,
        activity_status: Some("Running".to_string()),
        hook_token: Some("tok-label-unit".to_string()),
        sandbox: None,
        agent: None,
        recipe: None,
        agents: Vec::new(),
        agents_rev: 0,
        legacy_specialized_agents: Vec::new(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
        agent_daemon_instance_id: None,
        agent_session_id: None,
    }
}

/// A claude-cli `.session.yaml`, optionally recording the worktree the session works in. This
/// session type has no `changeset.yaml`, so the enrichment reports its `workflow_goal` as empty —
/// which is why the goal step of the rule is exercised by `write_tool_session` instead.
fn write_session(sessions_base: &Path, repo_path: Option<&str>) {
    let session_dir = sessions_base.join("sessions").join(SESSION_ID);
    std::fs::create_dir_all(&session_dir).unwrap();
    let meta = SessionMetadata {
        repo_path: repo_path.map(str::to_owned),
        ..a_session_metadata()
    };
    write_session_metadata(&session_dir, &meta).unwrap();
}

/// A tool session's `changeset.yaml`. Its `sessions[].tag` is what the enrichment reports as
/// `workflow_goal`, and therefore what the label rule falls back to when no worktree is recorded.
/// Omitting the row for this session is how the enrichment comes to report the `—` placeholder.
fn write_tool_session(
    sessions_base: &Path,
    repo_path: Option<&str>,
    goal_for_this_session: Option<&str>,
) {
    let session_dir = sessions_base.join("sessions").join(SESSION_ID);
    std::fs::create_dir_all(&session_dir).unwrap();
    let meta = SessionMetadata {
        session_type: Some("tool".to_string()),
        repo_path: repo_path.map(str::to_owned),
        ..a_session_metadata()
    };
    write_session_metadata(&session_dir, &meta).unwrap();

    let session_row = match goal_for_this_session {
        Some(tag) => format!(
            "  - id: {SESSION_ID}\n    agent: claude\n    tag: {tag}\n    created_at: '2026-08-29T10:00:00Z'\n"
        ),
        None => "  - id: some-other-session\n    agent: claude\n    tag: plan\n    created_at: '2026-08-29T10:00:00Z'\n".to_string(),
    };
    // `version`, `models`, `sessions` and `state` are all required by `Changeset` — omit any one
    // and `read_changeset` fails, the enrichment returns its all-placeholder row, and a test that
    // meant to exercise the goal step would silently be exercising the fallback instead.
    std::fs::write(
        session_dir.join("changeset.yaml"),
        format!(
            "version: 1\nmodels:\n  plan: sonnet-4\nsessions:\n{session_row}state:\n  current: Red\n  updated_at: '2026-08-29T10:00:00Z'\n  history: []\n"
        ),
    )
    .unwrap();
}

#[test]
fn names_a_session_after_the_worktree_recorded_in_its_metadata() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    write_session(sessions_tmp.path(), Some("/home/dev/my-feature-branch"));

    // When
    let label = resolve_session_label(sessions_tmp.path(), SESSION_ID);

    // Then
    assert_eq!(label, "my-feature-branch");
}

/// A claude-cli session has no workflow goal at all — `session_list_enrichment` reports it as the
/// empty string for this session type — so with no worktree recorded either, the id is all that is
/// left to name it by.
#[test]
fn falls_back_to_the_short_session_id_when_the_session_records_neither_worktree_nor_goal() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    write_session(sessions_tmp.path(), None);

    // When
    let label = resolve_session_label(sessions_tmp.path(), SESSION_ID);

    // Then
    assert_eq!(label, "01900000");
}

/// A hook can outrace the session directory's creation, and a deleted session can still have an
/// in-flight report. Neither may panic, and neither may produce an empty label.
#[test]
fn falls_back_to_the_short_session_id_when_the_session_directory_is_missing() {
    // Given — nothing on disk at all
    let sessions_tmp = tempfile::tempdir().unwrap();

    // When
    let label = resolve_session_label(sessions_tmp.path(), SESSION_ID);

    // Then
    assert_eq!(label, "01900000");
}

#[test]
fn ignores_a_worktree_path_recorded_as_the_empty_string() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    write_session(sessions_tmp.path(), Some(""));

    // When
    let label = resolve_session_label(sessions_tmp.path(), SESSION_ID);

    // Then
    assert_eq!(label, "01900000");
}

/// The middle step of the three-step rule. A tool session records its goal in `changeset.yaml`,
/// and with no worktree to be named after, that goal is the label — the same value, from the same
/// place, that `ListSessions` puts in front of the drawer.
#[test]
fn names_a_session_after_its_workflow_goal_when_it_records_no_worktree() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    write_tool_session(sessions_tmp.path(), None, Some("acceptance-tests"));

    // When
    let label = resolve_session_label(sessions_tmp.path(), SESSION_ID);

    // Then
    assert_eq!(label, "acceptance-tests");
}

/// A changeset that does not list this session makes the enrichment report the display placeholder
/// `—`. Naming a chat message "Session —" would be worse than the uuid it replaces, so the
/// placeholder counts as absent here exactly as it does in the drawer.
#[test]
fn falls_back_to_the_short_session_id_when_the_enrichment_reports_the_display_placeholder() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    write_tool_session(sessions_tmp.path(), None, None);

    // When
    let label = resolve_session_label(sessions_tmp.path(), SESSION_ID);

    // Then
    assert_eq!(label, "01900000");
}

/// The worktree still outranks a goal that is genuinely present.
#[test]
fn prefers_the_worktree_over_a_workflow_goal_when_the_session_records_both() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    write_tool_session(
        sessions_tmp.path(),
        Some("/home/dev/my-feature-branch"),
        Some("acceptance-tests"),
    );

    // When
    let label = resolve_session_label(sessions_tmp.path(), SESSION_ID);

    // Then
    assert_eq!(label, "my-feature-branch");
}
