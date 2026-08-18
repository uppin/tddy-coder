//! Acceptance tests: Cursor Agent CLI session type (PRD: docs/ft/daemon/cursor-cli-session.md).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tddy_core::session_metadata::{read_session_metadata, SessionMetadata};
use tddy_daemon::claude_cli_session::CliSessionManager;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::cursor_cli_spawn::spawn_cursor_cli_session_inner;
use tddy_daemon::session_room::{OpenedSessionRoom, SessionRoomHost};
use tddy_rpc::{Code, Request, Response, Status};
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListSessionsRequest, StartSessionRequest,
    StartSessionResponse,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const TEST_MODEL: &str = "claude-4.6-sonnet-medium-thinking";
const TEST_PROJECT_ID: &str = "test-project";

/// The chat id the stub `cursor-agent` mints for `create-chat`.
const STUB_CHAT_ID: &str = "f8db82db-e154-41d0-ae72-312bdf6d4d80";

fn write_config_with_cursor_cli_binary(stub_binary: &str) -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let config = load_cursor_cli_config(dir.path(), stub_binary);
    (dir, config)
}

fn load_cursor_cli_config(dir: &std::path::Path, stub_binary: &str) -> DaemonConfig {
    let yaml = format!(
        r#"
users:
  - github_user: "testuser"
    os_user: "testuser"
allowed_tools:
  - path: /bin/true
    label: true
cursor_cli:
  binary_path: {stub_binary}
"#
    );
    let config_path = dir.join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    DaemonConfig::load(&config_path).expect("config must parse")
}

/// Write a stub `cursor-agent` that mints [`STUB_CHAT_ID`] and otherwise idles on stdin.
fn write_stub_cursor_agent(dir: &std::path::Path) -> std::path::PathBuf {
    let script_path = dir.join("stub_cursor_agent.sh");
    std::fs::write(
        &script_path,
        format!(
            "#!/bin/sh\nif [ \"$1\" = \"create-chat\" ]; then echo \"{STUB_CHAT_ID}\"; exit 0; fi\ncat\n"
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script_path
}

/// A daemon config whose cursor binary is a stub that can mint a chat id.
///
/// A cursor-cli start now runs `cursor-agent create-chat` before launching the agent, so a plain
/// `/bin/cat` no longer stands in: it exits non-zero on `create-chat` and the start fails.
fn write_config_with_stub_cursor_agent() -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let stub = write_stub_cursor_agent(dir.path());
    let config = load_cursor_cli_config(dir.path(), stub.to_str().unwrap());
    (dir, config)
}

fn minimal_service(config: DaemonConfig, sessions_base: PathBuf) -> ConnectionServiceImpl {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver = Arc::new(|token| {
        if token == VALID_TOKEN {
            Some("testuser".to_string())
        } else {
            None
        }
    });
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        Arc::new(CliSessionManager::new()),
    )
}

fn create_test_repo_with_origin(dir: &std::path::Path) {
    let run = |args: &[&str], envs: &[(&str, &str)]| {
        let mut cmd = std::process::Command::new("git");
        cmd.args(args).current_dir(dir);
        for (k, v) in envs {
            cmd.env(k, v);
        }
        cmd.output().expect("git command failed");
    };
    let author_env = &[
        ("GIT_AUTHOR_NAME", "Test"),
        ("GIT_AUTHOR_EMAIL", "t@t.com"),
        ("GIT_COMMITTER_NAME", "Test"),
        ("GIT_COMMITTER_EMAIL", "t@t.com"),
    ];
    run(&["init", "-b", "main"], &[]);
    run(&["config", "user.email", "t@t.com"], &[]);
    run(&["config", "user.name", "Test"], &[]);
    run(&["commit", "--allow-empty", "-m", "init"], author_env);
    run(&["remote", "add", "origin", dir.to_str().unwrap()], &[]);
    run(&["push", "-u", "origin", "main"], &[]);
}

/// A stub agent that echoes its argv, and mints [`STUB_CHAT_ID`] when asked for a chat.
fn write_echo_argv_script(dir: &std::path::Path) -> std::path::PathBuf {
    let script_path = dir.join("stub_agent.sh");
    std::fs::write(
        &script_path,
        format!(
            "#!/bin/sh\nif [ \"$1\" = \"create-chat\" ]; then echo \"{STUB_CHAT_ID}\"; exit 0; fi\necho \"ARGV: $@\"\ncat\n"
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script_path
}

fn register_project(projects_dir: &std::path::Path, repo_path: &std::path::Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: test-project\n    git_url: \"\"\n    main_repo_path: {}\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

fn start_cursor_cli_request() -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        tool_path: String::new(),
        project_id: TEST_PROJECT_ID.to_string(),
        agent: String::new(),
        daemon_instance_id: String::new(),
        recipe: String::new(),
        session_type: "cursor-cli".to_string(),
        model: TEST_MODEL.to_string(),
        branch_worktree_intent: String::new(),
        new_branch_name: String::new(),
        selected_integration_base_ref: String::new(),
        selected_branch_to_work_on: String::new(),
        initial_prompt: String::new(),
        permission_mode: String::new(),
        stack_parent: String::new(),
        sandbox: false,
        managed_codebase: false,
        specialized_agents: vec![],
        ..Default::default()
    }
}

#[test]
fn build_cursor_argv_includes_chat_model_and_optional_prompt() {
    // Given / When
    let argv = CliSessionManager::build_cursor_argv(
        "/usr/bin/agent",
        "gpt-5.3-codex",
        Some("f8db82db-e154-41d0-ae72-312bdf6d4d80"),
        Some("fix the bug"),
    );

    // Then
    assert_eq!(
        argv,
        vec![
            "/usr/bin/agent".to_string(),
            "--resume".to_string(),
            "f8db82db-e154-41d0-ae72-312bdf6d4d80".to_string(),
            "--model".to_string(),
            "gpt-5.3-codex".to_string(),
            "fix the bug".to_string(),
        ]
    );
}

#[tokio::test]
async fn cursor_cli_start_with_empty_branch_name_uses_default_branch() {
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let (_cfg_dir, config) = write_config_with_stub_cursor_agent();
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let mut req = start_cursor_cli_request();
    req.branch_worktree_intent = "new_branch_from_base".to_string();
    req.new_branch_name = String::new();

    let resp = service
        .start_session(Request::new(req))
        .await
        .expect("StartSession must succeed with web-form branch defaults");

    let session_id = resp.into_inner().session_id;
    let short_id = &session_id[..8.min(session_id.len())];
    let session_dir = sessions_tmp.path().join("sessions").join(&session_id);
    let cs = tddy_core::read_changeset(&session_dir).expect("changeset must exist");
    let branch = cs
        .workflow
        .and_then(|w| w.new_branch_name)
        .expect("default branch name must be set");
    assert_eq!(branch, format!("cursor-cli/{short_id}"));
}

#[tokio::test]
async fn cursor_cli_session_metadata_fields_persisted() {
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let (_cfg_dir, config) = write_config_with_stub_cursor_agent();
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let resp = service
        .start_session(Request::new(start_cursor_cli_request()))
        .await
        .expect("StartSession cursor-cli must succeed");

    let session_id = resp.into_inner().session_id;
    let session_dir = sessions_tmp.path().join("sessions").join(&session_id);
    let meta = read_session_metadata(&session_dir).expect(".session.yaml must exist");

    assert_eq!(meta.session_type.as_deref(), Some("cursor-cli"));
    assert_eq!(meta.model.as_deref(), Some(TEST_MODEL));
    assert!(meta.hook_token.is_some());
    assert!(meta
        .repo_path
        .as_ref()
        .is_some_and(|p| PathBuf::from(p).exists()));
}

#[tokio::test]
async fn cursor_cli_session_writes_hooks_json() {
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let (_cfg_dir, config) = write_config_with_stub_cursor_agent();
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let resp = service
        .start_session(Request::new(start_cursor_cli_request()))
        .await
        .expect("StartSession must succeed");

    let session_id = resp.into_inner().session_id;
    let meta =
        read_session_metadata(&sessions_tmp.path().join("sessions").join(&session_id)).unwrap();
    let worktree = PathBuf::from(meta.repo_path.unwrap());
    let hooks_path = worktree.join(".cursor/hooks.json");
    assert!(hooks_path.exists(), "hooks.json must be written");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(hooks_path).unwrap()).unwrap();
    assert_eq!(json.get("version").and_then(|v| v.as_i64()), Some(1));
    assert!(json["hooks"]["sessionStart"].is_array());
}

#[tokio::test]
async fn cursor_cli_sandbox_start_succeeds_when_sandbox_backend_available() {
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        return;
    }
    #[cfg(target_os = "linux")]
    if !tddy_sandbox_cgroups::unprivileged_userns_available() {
        return;
    }

    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let stub = write_echo_argv_script(repo_dir.path());
    let (_cfg_dir, config) = write_config_with_cursor_cli_binary(stub.to_str().unwrap());
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let mut req = start_cursor_cli_request();
    req.sandbox = true;
    let resp = service
        .start_session(Request::new(req))
        .await
        .expect("sandbox cursor-cli must start when sandbox backend is available");

    assert!(resp.into_inner().livekit_room.is_empty());
}

#[tokio::test]
async fn cursor_cli_session_enrichment_reads_from_metadata() {
    let sessions_tmp = tempfile::tempdir().unwrap();
    let session_id = "01900000-0000-7000-8000-000000000099";
    let session_dir = sessions_tmp
        .path()
        .join("testuser")
        .join("sessions")
        .join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();

    let meta = SessionMetadata {
        session_id: session_id.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some("/tmp/worktree-stub".to_string()),
        pid: Some(99999),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("cursor-cli".to_string()),
        model: Some(TEST_MODEL.to_string()),
        cursor_chat_id: None,
        activity_status: None,
        hook_token: None,
        sandbox: None,
        agent: None,
        recipe: None,
        agents: Vec::new(),
        agents_rev: 0,
        legacy_specialized_agents: Vec::new(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
    };
    tddy_core::write_session_metadata(&session_dir, &meta).unwrap();

    let (_cfg_dir, config) = write_config_with_stub_cursor_agent();
    let sessions_base = sessions_tmp.path().join("testuser");
    let service = minimal_service(config, sessions_base);

    let list = service
        .list_sessions(Request::new(ListSessionsRequest {
            session_token: VALID_TOKEN.to_string(),
        }))
        .await
        .expect("ListSessions must succeed")
        .into_inner();

    let entry = list
        .sessions
        .iter()
        .find(|s| s.session_id == session_id)
        .expect("session must appear in list");
    assert_eq!(entry.agent, "cursor-cli");
    assert_eq!(entry.model, TEST_MODEL);
}

// ---------------------------------------------------------------------------
// Peer-agent spawn ("Add agent") — reusing the orchestrator's worktree via
// `repo_path` and recording the `stack_parent` link.
//
// These mirror the contract the claude-cli sandboxed path already honors
// (`start_sandboxed_claude_cli_session` → `session_worktree_source` +
// `orchestrator_session_id: stack_parent.map(...)`). The cursor-cli path must
// honor the same two fields so a peer cursor-cli child of a claude-cli (or
// cursor-cli) orchestrator runs on the SAME worktree and is linked back to the
// orchestrator, instead of becoming a standalone session on a fresh branch.
// ---------------------------------------------------------------------------

/// Build a peer-spawn `StartSessionRequest`: `session_type = "cursor-cli"`,
/// `repo_path` set to the orchestrator's worktree, `stack_parent` set to the
/// orchestrator's session id, and `branch_worktree_intent` empty (the web
/// `CreateSessionPane` peer mode sends exactly these — see
/// `packages/tddy-web/src/components/sessions/CreateSessionPane.tsx`).
fn peer_cursor_cli_request(repo_path: &str, stack_parent: &str) -> StartSessionRequest {
    let mut req = start_cursor_cli_request();
    req.repo_path = repo_path.to_string();
    req.stack_parent = stack_parent.to_string();
    req.branch_worktree_intent = String::new();
    req.new_branch_name = String::new();
    req.selected_branch_to_work_on = String::new();
    req
}

/// `git -C <repo> branch --list <pattern>` → the matching branch names, one per line, trimmed.
fn local_branches_matching(repo: &std::path::Path, pattern: &str) -> Vec<String> {
    let output = std::process::Command::new("git")
        .current_dir(repo)
        .args(["branch", "--list", pattern])
        .output()
        .expect("git branch --list must run");
    assert!(output.status.success(), "git branch --list failed");
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|l| l.trim().trim_start_matches("* ").trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[tokio::test]
async fn cursor_cli_peer_spawn_reuses_the_orchestrator_worktree_when_repo_path_is_set() {
    // Given — a registered project repo, and a separate pre-existing checkout
    // that the orchestrating session already runs in (the peer must reuse it).
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let (_cfg_dir, config) = write_config_with_stub_cursor_agent();
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let orchestrator_worktree = tempfile::tempdir().unwrap();
    let orchestrator_session_id = "019f9fdb-cf83-70d2-aef5-062db0db7e75";
    let orchestrator_worktree_canonical =
        std::fs::canonicalize(orchestrator_worktree.path()).unwrap();

    // When — a peer "Add agent" spawn pointing at the orchestrator's worktree
    let resp = service
        .start_session(Request::new(peer_cursor_cli_request(
            orchestrator_worktree.path().to_str().unwrap(),
            orchestrator_session_id,
        )))
        .await
        .expect("peer cursor-cli StartSession must succeed");

    // Then — the session runs on the orchestrator's worktree, not a fresh one
    let session_id = resp.into_inner().session_id;
    let session_dir = sessions_tmp.path().join("sessions").join(&session_id);
    let meta = read_session_metadata(&session_dir).expect(".session.yaml must exist");
    let recorded_worktree =
        std::fs::canonicalize(meta.repo_path.as_deref().expect("repo_path must be set")).unwrap();
    assert_eq!(
        recorded_worktree, orchestrator_worktree_canonical,
        "peer cursor-cli must reuse the orchestrator's worktree via repo_path, \
         not create a new daemon-managed worktree"
    );

    // And — the orchestrator link is recorded on the changeset
    let cs = tddy_core::read_changeset(&session_dir).expect("changeset must exist");
    assert_eq!(
        cs.orchestrator_session_id.as_deref(),
        Some(orchestrator_session_id),
        "peer cursor-cli must record orchestrator_session_id from stack_parent"
    );

    // And — no daemon-managed worktree was created under the project repo
    let project_worktrees = repo_dir.path().join(".worktrees");
    assert!(
        !project_worktrees.exists()
            || std::fs::read_dir(&project_worktrees)
                .unwrap()
                .next()
                .is_none(),
        "peer cursor-cli with repo_path set must not create a worktree under the project repo"
    );
}

/// An orchestrating session on disk for a peer's `stack_parent` to name: a code session on `main`
/// in the project repo the peer is spawned into.
///
/// A peer that creates its own branch bases it off the parent's, which is read from the parent's
/// changeset — so the parent has to exist. A `stack_parent` that resolves to nothing is refused
/// (`FailedPrecondition`) rather than quietly based off the default branch, which is why this
/// fixture writes a real one instead of the id alone.
fn an_orchestrator_session(
    sessions_base: &std::path::Path,
    session_id: &str,
    repo: &std::path::Path,
) {
    let session_dir = sessions_base.join("sessions").join(session_id);
    std::fs::create_dir_all(&session_dir).expect("create the orchestrator's session dir");
    tddy_core::changeset::write_changeset(
        &session_dir,
        &tddy_core::changeset::Changeset {
            branch: Some("main".to_string()),
            repo_path: Some(repo.to_string_lossy().into_owned()),
            ..Default::default()
        },
    )
    .expect("write the orchestrator's changeset");
}

#[tokio::test]
async fn cursor_cli_peer_spawn_records_the_orchestrator_link_even_without_repo_path() {
    // Given — a registered project repo, an orchestrating session to chain from, and no client
    // repo_path (standalone worktree path)
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let (_cfg_dir, config) = write_config_with_stub_cursor_agent();
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let orchestrator_session_id = "019f9dd5-716d-7071-96ac-464ff7b98c2a";
    an_orchestrator_session(
        sessions_tmp.path(),
        orchestrator_session_id,
        repo_dir.path(),
    );

    // When — a cursor-cli spawn that carries only a stack_parent (no repo_path),
    // creating its own worktree as before but still linked to the orchestrator
    let mut req = start_cursor_cli_request();
    req.stack_parent = orchestrator_session_id.to_string();
    req.branch_worktree_intent = "new_branch_from_base".to_string();
    let resp = service
        .start_session(Request::new(req))
        .await
        .expect("cursor-cli StartSession with stack_parent must succeed");

    // Then — the orchestrator link is recorded even though repo_path was empty
    let session_id = resp.into_inner().session_id;
    let session_dir = sessions_tmp.path().join("sessions").join(&session_id);
    let cs = tddy_core::read_changeset(&session_dir).expect("changeset must exist");
    assert_eq!(
        cs.orchestrator_session_id.as_deref(),
        Some(orchestrator_session_id),
        "cursor-cli must record orchestrator_session_id from stack_parent even without repo_path"
    );
}

#[tokio::test]
async fn cursor_cli_peer_spawn_with_repo_path_creates_no_new_branch_in_the_project_repo() {
    // Given — a registered project repo with an origin, and an orchestrator worktree
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let (_cfg_dir, config) = write_config_with_stub_cursor_agent();
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let orchestrator_worktree = tempfile::tempdir().unwrap();

    // When — a peer spawn reusing the orchestrator's worktree
    let resp = service
        .start_session(Request::new(peer_cursor_cli_request(
            orchestrator_worktree.path().to_str().unwrap(),
            "orchestrator-branch-test",
        )))
        .await
        .expect("peer cursor-cli StartSession must succeed");

    // Then — no `cursor-cli/*` branch was created in the project repo
    let _ = resp;
    let branches = local_branches_matching(repo_dir.path(), "cursor-cli/*");
    assert!(
        branches.is_empty(),
        "peer cursor-cli with repo_path set must not create a cursor-cli/* branch in the project repo, \
         got: {branches:?}"
    );
}

#[tokio::test]
async fn cursor_cli_peer_spawn_rejects_a_repo_path_that_is_not_a_directory() {
    // Given — a registered project repo and a regular file masquerading as repo_path
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let (_cfg_dir, config) = write_config_with_stub_cursor_agent();
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let file_dir = tempfile::tempdir().unwrap();
    let file_path = file_dir.path().join("not-a-dir.txt");
    std::fs::write(&file_path, "I am a file").unwrap();

    // When / Then — StartSession rejects the file-as-repo_path with INVALID_ARGUMENT
    let err = service
        .start_session(Request::new(peer_cursor_cli_request(
            file_path.to_str().unwrap(),
            "orchestrator-file-repo-path",
        )))
        .await
        .expect_err("cursor-cli StartSession with a non-directory repo_path must fail");
    assert_eq!(
        err.code,
        Code::InvalidArgument,
        "a non-directory repo_path must yield INVALID_ARGUMENT"
    );
    assert!(
        err.message.to_ascii_lowercase().contains("not a directory"),
        "error message must explain the repo_path is not a directory, got: {}",
        err.message
    );
}

// ---------------------------------------------------------------------------
// Session room (PRD: docs/ft/daemon/session-room.md).
//
// A cursor-cli session runs its agent on this daemon, against a checkout this
// daemon made — so this daemon is its facilitating daemon and hosts its room,
// exactly as it does for a claude-cli session. The room is opened before the
// agent process exists, which is what makes the daemon its first participant.
// ---------------------------------------------------------------------------

/// One `open_for` call, as the spawn path made it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AnOpenedRoom {
    session_id: String,
    worktree_root: PathBuf,
    session_dir: PathBuf,
}

/// A [`SessionRoomHost`] that records what it was asked to host, standing in for the daemon's own —
/// which needs a LiveKit server to answer at all.
#[derive(Default)]
struct RecordingRoomHost {
    opened: Mutex<Vec<AnOpenedRoom>>,
    /// When set, the reason this host cannot open a room: a configured daemon that fails to reach
    /// its LiveKit server answers this way.
    refusal: Option<String>,
}

/// A daemon that hosts session rooms and records the ones it opens.
fn a_room_host() -> RecordingRoomHost {
    RecordingRoomHost::default()
}

impl RecordingRoomHost {
    /// A daemon configured to host rooms that cannot reach LiveKit to open one.
    fn that_cannot_open_a_room(mut self) -> Self {
        self.refusal = Some("livekit is unreachable".to_string());
        self
    }

    fn rooms_opened(&self) -> Vec<AnOpenedRoom> {
        self.opened
            .lock()
            .expect("room log is not poisoned")
            .clone()
    }
}

#[async_trait::async_trait]
impl SessionRoomHost for RecordingRoomHost {
    async fn open_for(
        &self,
        session_id: &str,
        worktree_root: &Path,
        session_dir: &Path,
    ) -> Result<Option<OpenedSessionRoom>, Status> {
        if let Some(reason) = &self.refusal {
            return Err(Status::internal(reason.clone()));
        }
        self.opened
            .lock()
            .expect("room log is not poisoned")
            .push(AnOpenedRoom {
                session_id: session_id.to_string(),
                worktree_root: worktree_root.to_path_buf(),
                session_dir: session_dir.to_path_buf(),
            });
        Ok(Some(OpenedSessionRoom {
            room: format!("session-{session_id}"),
            url: "ws://livekit.test".to_string(),
            server_identity: "daemon-test-instance".to_string(),
        }))
    }
}

/// A daemon that can start cursor-cli sessions: a project repo with an origin, a sessions base with
/// that project registered, and a config pointing at a stub `cursor-agent`.
struct ACursorCliDaemon {
    _repo: tempfile::TempDir,
    _config_dir: tempfile::TempDir,
    sessions: tempfile::TempDir,
    config: DaemonConfig,
    agents: Arc<CliSessionManager>,
}

fn a_cursor_cli_daemon() -> ACursorCliDaemon {
    let repo = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo.path());
    let sessions = tempfile::tempdir().unwrap();
    register_project(&sessions.path().join("projects"), repo.path());
    let (config_dir, config) = write_config_with_stub_cursor_agent();
    ACursorCliDaemon {
        _repo: repo,
        _config_dir: config_dir,
        sessions,
        config,
        agents: Arc::new(CliSessionManager::new()),
    }
}

impl ACursorCliDaemon {
    /// Start a cursor-cli session at the spawn helper itself, so the room host is the test's
    /// instead of the one `ConnectionServiceImpl` builds for itself out of its LiveKit config.
    async fn start_cursor_cli_session(
        &self,
        session_id: &str,
        room_host: &dyn SessionRoomHost,
    ) -> Result<Response<StartSessionResponse>, Status> {
        spawn_cursor_cli_session_inner(
            &self.config,
            self.sessions.path(),
            &self.agents,
            "testuser",
            session_id,
            self.sessions.path().to_path_buf(),
            TEST_MODEL,
            TEST_PROJECT_ID,
            "new_branch_from_base",
            "",
            "",
            "",
            "",
            None,
            "",
            false,
            &[],
            None,
            false,
            false,
            &self.agents.task_registry(),
            room_host,
        )
        .await
    }

    fn session_dir(&self, session_id: &str) -> PathBuf {
        self.sessions.path().join("sessions").join(session_id)
    }

    /// The checkout the started session recorded for itself in `.session.yaml`.
    fn worktree_of(&self, session_id: &str) -> PathBuf {
        let meta = read_session_metadata(&self.session_dir(session_id))
            .expect(".session.yaml must exist for a started session");
        PathBuf::from(
            meta.repo_path
                .expect("a started session records its worktree"),
        )
    }
}

const ROOM_SESSION_ID: &str = "019f9fdb-cf83-70d2-aef5-0000000000a1";

#[tokio::test]
async fn cursor_cli_start_hosts_the_room_of_the_session_it_starts() {
    // Given
    let daemon = a_cursor_cli_daemon();
    let rooms = a_room_host();

    // When
    daemon
        .start_cursor_cli_session(ROOM_SESSION_ID, &rooms)
        .await
        .expect("cursor-cli StartSession must succeed");

    // Then
    assert_eq!(
        rooms.rooms_opened(),
        vec![AnOpenedRoom {
            session_id: ROOM_SESSION_ID.to_string(),
            worktree_root: daemon.worktree_of(ROOM_SESSION_ID),
            session_dir: daemon.session_dir(ROOM_SESSION_ID),
        }],
        "a cursor-cli session must be facilitated in a room of its own, over its own checkout"
    );
}

#[tokio::test]
async fn cursor_cli_start_fails_when_the_session_room_cannot_be_opened() {
    // Given
    let daemon = a_cursor_cli_daemon();
    let rooms = a_room_host().that_cannot_open_a_room();

    // When
    let err = daemon
        .start_cursor_cli_session(ROOM_SESSION_ID, &rooms)
        .await
        .expect_err("a session whose room cannot be opened must not start");

    // Then
    assert_eq!(err.message, "livekit is unreachable");
}

#[tokio::test]
async fn cursor_cli_agent_is_not_spawned_when_the_session_room_cannot_be_opened() {
    // Given
    let daemon = a_cursor_cli_daemon();
    let rooms = a_room_host().that_cannot_open_a_room();

    // When
    let _ = daemon
        .start_cursor_cli_session(ROOM_SESSION_ID, &rooms)
        .await;

    // Then — the room is opened first, so a refused room leaves no agent behind
    assert!(
        daemon.agents.get(ROOM_SESSION_ID).await.is_none(),
        "the cursor agent must not be spawned before its session room is open"
    );
}

#[tokio::test]
async fn cursor_cli_peer_spawn_rejects_a_missing_repo_path() {
    // Given — a registered project repo and a repo_path that does not exist
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let (_cfg_dir, config) = write_config_with_stub_cursor_agent();
    let service = minimal_service(config, sessions_tmp.path().to_path_buf());

    let missing_path = sessions_tmp.path().join("does-not-exist");

    // When / Then — StartSession rejects the missing repo_path with INVALID_ARGUMENT
    let err = service
        .start_session(Request::new(peer_cursor_cli_request(
            missing_path.to_str().unwrap(),
            "orchestrator-missing-repo-path",
        )))
        .await
        .expect_err("cursor-cli StartSession with a missing repo_path must fail");
    assert_eq!(
        err.code,
        Code::InvalidArgument,
        "a missing repo_path must yield INVALID_ARGUMENT"
    );
    assert!(
        err.message.to_ascii_lowercase().contains("not accessible"),
        "error message must explain the repo_path is not accessible, got: {}",
        err.message
    );
}
