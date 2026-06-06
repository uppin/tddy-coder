//! Acceptance tests: Claude Code CLI session type (PRD: docs/ft/daemon/claude-cli-session.md).
//!
//! These tests define the desired behaviour of `session_type = "claude-cli"` sessions:
//! session metadata persistence, enrichment without changeset.yaml, LiveKit-free responses,
//! and resume in the existing worktree.
//!
//! All tests currently fail because the implementation does not yet exist:
//! - `StartSessionRequest` has no `session_type` / `model` fields (proto not yet extended)
//! - `SessionMetadata` has no `session_type` / `model` fields (`deny_unknown_fields` guards)
//! - `ConnectionServiceImpl::start_session` does not branch on `session_type = "claude-cli"`
//! - `session_list_enrichment` always falls back to `all_placeholders()` when changeset.yaml is
//!   absent; it does not read `session_type`/`model` from `.session.yaml`

use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::session_metadata::{
    read_session_metadata, write_session_metadata, SessionMetadata, SESSION_METADATA_FILENAME,
};
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListSessionsRequest, ResumeSessionRequest,
    StartSessionRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const TEST_MODEL: &str = "claude-opus-4-8";
const TEST_PROJECT_ID: &str = "test-project";

fn write_config_with_claude_cli_binary(stub_binary: &str) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let yaml = format!(
        r#"
users:
  - github_user: "testuser"
    os_user: "testuser"
allowed_tools:
  - path: /bin/true
    label: true
claude_cli:
  binary_path: {stub_binary}
"#
    );
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    (dir, config)
}

fn minimal_service(config: DaemonConfig, sessions_base: PathBuf) -> ConnectionServiceImpl {
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver = Arc::new(|token| {
        if token == VALID_TOKEN {
            Some("testuser".to_string())
        } else {
            None
        }
    });
    ConnectionServiceImpl::new(config, sessions_base_resolver, user_resolver, None, None, None)
}

/// **claude_cli_session_metadata_fields_persisted**: after `StartSession` with
/// `session_type = "claude-cli"` succeeds, `.session.yaml` under the new session directory must
/// contain `session_type = "claude-cli"` and `model = TEST_MODEL`.
///
/// FAILS: `StartSessionRequest` has no `session_type` / `model` fields (proto not extended).
#[tokio::test]
async fn claude_cli_session_metadata_fields_persisted() {
    let repo_dir = tempfile::tempdir().unwrap();
    // Initialise as a git repo so the daemon can create a worktree.
    std::process::Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(repo_dir.path())
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "--allow-empty", "-m", "init"])
        .current_dir(repo_dir.path())
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .output()
        .unwrap();

    let sessions_tmp = tempfile::tempdir().unwrap();
    // `echo` as a stub for `claude` so the PTY spawns successfully without the real binary.
    let (_cfg_dir, config) = write_config_with_claude_cli_binary("/bin/cat");
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let resp = service
        .start_session(Request::new(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            tool_path: String::new(),
            project_id: TEST_PROJECT_ID.to_string(),
            agent: String::new(),
            daemon_instance_id: String::new(),
            recipe: String::new(),
            session_type: "claude-cli".to_string(), // NEW FIELD — compile error until proto extended
            model: TEST_MODEL.to_string(),           // NEW FIELD — compile error until proto extended
        }))
        .await
        .expect("StartSession with session_type=claude-cli must succeed");

    let session_id = resp.into_inner().session_id;
    assert!(!session_id.is_empty(), "session_id must be non-empty");

    let session_dir = sessions_tmp
        .path()
        .join("testuser")
        .join("sessions")
        .join(&session_id);
    let meta = read_session_metadata(&session_dir)
        .expect(".session.yaml must be written for claude-cli session");

    assert_eq!(
        meta.session_type.as_deref(),
        Some("claude-cli"),
        "session_type must be persisted as 'claude-cli'"
    );
    assert_eq!(
        meta.model.as_deref(),
        Some(TEST_MODEL),
        "model must be persisted in .session.yaml"
    );
    assert!(
        meta.repo_path.is_some(),
        "repo_path must be set to the created worktree path"
    );
    let worktree_path = PathBuf::from(meta.repo_path.unwrap());
    assert!(
        worktree_path.exists(),
        "worktree directory must exist at repo_path: {}",
        worktree_path.display()
    );
}

/// **claude_cli_session_livekit_fields_empty**: `StartSessionResponse` for
/// `session_type = "claude-cli"` must return empty `livekit_room`, `livekit_url`, and
/// `livekit_server_identity` — no LiveKit room is created for these sessions.
///
/// FAILS: `StartSessionRequest` has no `session_type` / `model` fields (proto not extended).
#[tokio::test]
async fn claude_cli_session_livekit_fields_empty() {
    let repo_dir = tempfile::tempdir().unwrap();
    std::process::Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(repo_dir.path())
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "--allow-empty", "-m", "init"])
        .current_dir(repo_dir.path())
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .output()
        .unwrap();

    let sessions_tmp = tempfile::tempdir().unwrap();
    let (_cfg_dir, config) = write_config_with_claude_cli_binary("/bin/cat");
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let inner = service
        .start_session(Request::new(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            tool_path: String::new(),
            project_id: TEST_PROJECT_ID.to_string(),
            agent: String::new(),
            daemon_instance_id: String::new(),
            recipe: String::new(),
            session_type: "claude-cli".to_string(), // NEW FIELD
            model: TEST_MODEL.to_string(),           // NEW FIELD
        }))
        .await
        .expect("StartSession must succeed")
        .into_inner();

    assert!(
        inner.livekit_room.is_empty(),
        "livekit_room must be empty for claude-cli sessions; got: {}",
        inner.livekit_room
    );
    assert!(
        inner.livekit_url.is_empty(),
        "livekit_url must be empty for claude-cli sessions; got: {}",
        inner.livekit_url
    );
    assert!(
        inner.livekit_server_identity.is_empty(),
        "livekit_server_identity must be empty for claude-cli sessions; got: {}",
        inner.livekit_server_identity
    );
}

/// **claude_cli_session_enrichment_reads_from_metadata**: when a session directory contains
/// `.session.yaml` with `session_type = "claude-cli"` and no `changeset.yaml`, `ListSessions`
/// must return `agent = "claude-cli"` and `model` from the metadata — not placeholder dashes.
///
/// FAILS:
/// 1. `SessionMetadata` has `deny_unknown_fields` and no `session_type`/`model` fields →
///    writing the YAML below fails to deserialise, meaning the test fixture itself cannot be
///    constructed until the struct is extended.
/// 2. Even with the struct extended, `session_list_enrichment` returns `all_placeholders()` when
///    changeset.yaml is absent instead of reading from metadata.
#[tokio::test]
async fn claude_cli_session_enrichment_reads_from_metadata() {
    let sessions_tmp = tempfile::tempdir().unwrap();
    let session_id = "01900000-0000-7000-8000-000000000001";
    let session_dir = sessions_tmp
        .path()
        .join("testuser")
        .join("sessions")
        .join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();

    // Write a .session.yaml with session_type and model — new fields that do not yet exist on
    // SessionMetadata (deny_unknown_fields will reject this YAML until the struct is extended).
    let meta = SessionMetadata {
        session_id: session_id.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        created_at: "2026-06-06T10:00:00Z".to_string(),
        updated_at: "2026-06-06T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some("/tmp/worktree-stub".to_string()),
        pid: Some(99999),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()), // NEW FIELD — compile error until struct extended
        model: Some(TEST_MODEL.to_string()),           // NEW FIELD — compile error until struct extended
    };
    write_session_metadata(&session_dir, &meta).unwrap();
    // No changeset.yaml — intentionally absent to test the claude-cli fallback path.

    let config_yaml = r#"
users:
  - github_user: "testuser"
    os_user: "testuser"
"#;
    let cfg_dir = tempfile::tempdir().unwrap();
    let cfg_path = cfg_dir.path().join("d.yaml");
    std::fs::write(&cfg_path, config_yaml).unwrap();
    let config = DaemonConfig::load(&cfg_path).unwrap();

    let sessions_base = sessions_tmp.path().join("testuser");
    let service = minimal_service(config, sessions_base);

    let sessions = service
        .list_sessions(Request::new(ListSessionsRequest {
            session_token: VALID_TOKEN.to_string(),
        }))
        .await
        .expect("ListSessions must succeed")
        .into_inner()
        .sessions;

    let entry = sessions
        .iter()
        .find(|s| s.session_id == session_id)
        .expect("session must appear in ListSessions response");

    assert_eq!(
        entry.agent, "claude-cli",
        "agent must be 'claude-cli' for claude-cli sessions (not '—')"
    );
    assert_eq!(
        entry.model, TEST_MODEL,
        "model must be read from .session.yaml for claude-cli sessions (not '—')"
    );
    assert!(
        entry.workflow_goal.is_empty() || entry.workflow_goal == "—",
        "workflow_goal should be empty/dash for claude-cli sessions (no changeset)"
    );
    assert!(
        entry.workflow_state.is_empty() || entry.workflow_state == "—",
        "workflow_state should be empty/dash for claude-cli sessions (no changeset)"
    );
}

/// **claude_cli_session_resume_relaunches_in_worktree**: after a claude-cli session becomes
/// inactive (process exits), `ResumeSession` must relaunch `claude --model <model>` in the
/// existing worktree and mark the session active again.
///
/// FAILS: `ResumeSession` does not yet branch on `session_type = "claude-cli"`.
#[tokio::test]
async fn claude_cli_session_resume_relaunches_in_worktree() {
    let worktree_dir = tempfile::tempdir().unwrap();
    let sessions_tmp = tempfile::tempdir().unwrap();
    let session_id = "01900000-0000-7000-8000-000000000002";
    let session_dir = sessions_tmp
        .path()
        .join("testuser")
        .join("sessions")
        .join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();

    // Simulate an inactive claude-cli session (process previously exited).
    let meta = SessionMetadata {
        session_id: session_id.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        created_at: "2026-06-06T10:00:00Z".to_string(),
        updated_at: "2026-06-06T10:05:00Z".to_string(),
        status: "inactive".to_string(),
        repo_path: Some(worktree_dir.path().to_str().unwrap().to_string()),
        pid: None,
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()), // NEW FIELD
        model: Some(TEST_MODEL.to_string()),           // NEW FIELD
    };
    write_session_metadata(&session_dir, &meta).unwrap();

    let (_cfg_dir, config) = write_config_with_claude_cli_binary("/bin/cat");
    let sessions_base = sessions_tmp.path().join("testuser");
    let service = minimal_service(config, sessions_base);

    let resp = service
        .resume_session(Request::new(ResumeSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: session_id.to_string(),
        }))
        .await
        .expect("ResumeSession must succeed for an inactive claude-cli session");

    let inner = resp.into_inner();
    assert_eq!(
        inner.session_id, session_id,
        "ResumeSession must return the same session_id"
    );
    assert!(
        inner.livekit_room.is_empty(),
        "resumed claude-cli session must not return a livekit_room"
    );

    // After resume, metadata must be updated to active with a fresh PID.
    let updated_meta = read_session_metadata(&session_dir)
        .expect(".session.yaml must be readable after resume");
    assert_eq!(
        updated_meta.status, "active",
        "session must be marked active after resume"
    );
    assert!(
        updated_meta.pid.is_some(),
        "session must have a non-None pid after resume (new PTY process spawned)"
    );
}

/// **claude_cli_start_session_requires_model**: `StartSession` with `session_type = "claude-cli"`
/// and an empty `model` must return `INVALID_ARGUMENT`.
///
/// FAILS: validation logic does not yet exist.
#[tokio::test]
async fn claude_cli_start_session_requires_model() {
    let sessions_tmp = tempfile::tempdir().unwrap();
    let config_yaml = r#"
users:
  - github_user: "testuser"
    os_user: "testuser"
"#;
    let cfg_dir = tempfile::tempdir().unwrap();
    let cfg_path = cfg_dir.path().join("d.yaml");
    std::fs::write(&cfg_path, config_yaml).unwrap();
    let config = DaemonConfig::load(&cfg_path).unwrap();
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let err = service
        .start_session(Request::new(StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            tool_path: String::new(),
            project_id: TEST_PROJECT_ID.to_string(),
            agent: String::new(),
            daemon_instance_id: String::new(),
            recipe: String::new(),
            session_type: "claude-cli".to_string(), // NEW FIELD
            model: String::new(),                    // empty — must be rejected
        }))
        .await
        .expect_err("StartSession with claude-cli and empty model must fail");

    assert_eq!(
        err.code,
        Code::InvalidArgument,
        "empty model for claude-cli must yield INVALID_ARGUMENT"
    );
    let msg = err.message.to_ascii_lowercase();
    assert!(
        msg.contains("model"),
        "error message must mention 'model'; got: {}",
        err.message
    );
}
