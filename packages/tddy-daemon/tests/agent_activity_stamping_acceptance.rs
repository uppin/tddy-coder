//! Stamping an activity record with the checkout it was made against — AC1/AC2 of
//! `docs/ft/daemon/session-worktree-sync.md`.
//!
//! Real git repositories in temp directories and the real `ReportAgentActivity` handler: what a
//! record carries is decided by the checkout the session names, never by a fixture.
//!
//! The two fields exist for one reason — a patch is only applicable against the commit it was cut
//! from, and only to the files it was scoped to — so what is pinned here is that both are *true of
//! the worktree*: the commit is cross-checked against `git rev-parse`, and a checkout with no
//! readable HEAD leaves the field empty rather than being handed something that looks like a sha.

use std::path::{Path, PathBuf};
use std::process::Command;

use pretty_assertions::assert_eq;
use tddy_core::agent_activity::{read_agent_activity, AgentActivityRecord};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::SessionMetadata;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::test_util::{test_service, TEST_USER};
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ReportAgentActivityRequest,
};

/// The per-session secret the hook authenticates with. Any value works; it only has to match what
/// the session's metadata holds.
const HOOK_TOKEN: &str = "hook-token-for-stamping";

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// A session reporting agent activity, and the checkout its records are stamped against.
struct ReportingSession {
    service: ConnectionServiceImpl,
    session_id: String,
    session_dir: PathBuf,
    _sessions: tempfile::TempDir,
}

/// A checkout with one commit, standing in for a session worktree. Returns its HEAD.
fn a_git_worktree(root: &Path) -> String {
    git(root, &["init", "--initial-branch=main"]);
    git(root, &["config", "user.email", "agent@example.com"]);
    git(root, &["config", "user.name", "Agent"]);
    std::fs::create_dir_all(root.join("src")).expect("create src");
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write source");
    git(root, &["add", "-A"]);
    git(root, &["commit", "-m", "initial"]);
    git(root, &["rev-parse", "HEAD"]).trim().to_string()
}

fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("failed to run git {args:?}: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A claude-cli session whose `repo_path` is `worktree`, ready to be reported against.
fn a_session_checked_out_at(worktree: &Path) -> ReportingSession {
    let sessions = tempfile::tempdir().expect("sessions tempdir");
    let session_id = "1780828020298-stamping".to_string();
    let session_dir = unified_session_dir_path(sessions.path(), &session_id);
    std::fs::create_dir_all(&session_dir).expect("create session dir");
    tddy_core::write_session_metadata(&session_dir, &a_claude_cli_session(&session_id, worktree))
        .expect("write session metadata");

    ReportingSession {
        service: test_service(sessions.path().to_path_buf()),
        session_id,
        session_dir,
        _sessions: sessions,
    }
}

fn a_claude_cli_session(session_id: &str, worktree: &Path) -> SessionMetadata {
    SessionMetadata {
        session_id: session_id.to_string(),
        project_id: "project-under-stamping".to_string(),
        created_at: "2026-08-15T10:00:00Z".to_string(),
        updated_at: "2026-08-15T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some(worktree.display().to_string()),
        pid: None,
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: None,
        cursor_chat_id: None,
        activity_status: None,
        hook_token: Some(HOOK_TOKEN.to_string()),
        sandbox: None,
        agent: None,
        recipe: None,
        agents: Vec::new(),
        agents_rev: 0,
        legacy_specialized_agents: Vec::new(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
    }
}

/// Report one tool call through the hook RPC and return the row it persisted.
async fn a_recorded_call(
    session: &ReportingSession,
    tool_name: &str,
    input: serde_json::Value,
) -> AgentActivityRecord {
    session
        .service
        .report_agent_activity(Request::new(ReportAgentActivityRequest {
            session_id: session.session_id.clone(),
            hook_token: HOOK_TOKEN.to_string(),
            os_user: TEST_USER.to_string(),
            event: "PreToolUse".to_string(),
            tool_name: tool_name.to_string(),
            input_json: input.to_string(),
            result_json: String::new(),
            is_error: false,
            error_message: String::new(),
        }))
        .await
        .expect("ReportAgentActivity must accept a hook event");

    let mut rows = read_agent_activity(&session.session_dir).expect("read agent activity");
    assert_eq!(rows.len(), 1, "one reported call must persist one row");
    rows.remove(0)
}

// ---------------------------------------------------------------------------
// The commit a record was made against — AC1
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stamps_a_recorded_call_with_the_worktrees_current_head_commit() {
    // Given a session whose checkout sits on a known commit
    let worktree = tempfile::tempdir().expect("worktree tempdir");
    let head = a_git_worktree(worktree.path());
    let session = a_session_checked_out_at(worktree.path());

    // When the agent reports a tool call
    let record = a_recorded_call(
        &session,
        "Edit",
        serde_json::json!({ "file_path": "src/main.rs" }),
    )
    .await;

    // Then the record names that commit
    assert_eq!(record.head_commit, head);
}

#[tokio::test]
async fn leaves_head_commit_empty_when_the_checkout_has_no_readable_head() {
    // Given a session whose `repo_path` is a directory, but not a git checkout
    let not_a_checkout = tempfile::tempdir().expect("worktree tempdir");
    let session = a_session_checked_out_at(not_a_checkout.path());

    // When the agent reports a tool call
    let record = a_recorded_call(
        &session,
        "Edit",
        serde_json::json!({ "file_path": "src/main.rs" }),
    )
    .await;

    // Then the record carries no commit rather than an invented one
    assert_eq!(record.head_commit, "");
}

// ---------------------------------------------------------------------------
// The paths a call is credited with — AC2
// ---------------------------------------------------------------------------

#[tokio::test]
async fn credits_an_edit_with_the_worktree_relative_path_it_declared() {
    // Given a session on a real checkout, and an Edit naming a file by its absolute path
    let worktree = tempfile::tempdir().expect("worktree tempdir");
    a_git_worktree(worktree.path());
    let session = a_session_checked_out_at(worktree.path());
    let edited = worktree.path().join("src/main.rs");

    // When the agent reports that Edit
    let record = a_recorded_call(
        &session,
        "Edit",
        serde_json::json!({ "file_path": edited.display().to_string() }),
    )
    .await;

    // Then the record credits it with that file, relative to the worktree
    assert_eq!(record.changed_paths, vec!["src/main.rs".to_string()]);
}

#[tokio::test]
async fn credits_a_bash_call_with_no_paths_because_it_declared_none() {
    // Given a session on a real checkout, and a Bash call that names a path it only reads
    let worktree = tempfile::tempdir().expect("worktree tempdir");
    a_git_worktree(worktree.path());
    let session = a_session_checked_out_at(worktree.path());

    // When the agent reports that Bash call
    let record = a_recorded_call(
        &session,
        "Bash",
        serde_json::json!({ "command": "cargo fmt", "file_path": "src/main.rs" }),
    )
    .await;

    // Then it is credited with nothing: whatever it changed reaches a consumer as the tick's
    // residual, never as this call's own patch.
    assert_eq!(record.changed_paths, Vec::<String>::new());
}
