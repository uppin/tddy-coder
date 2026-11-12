//! Acceptance: an **orchestrator** session is served main's reach through the exec tools — main's
//! tool set, main's read-only refusal, `StartSession` (which may itself ask for the orchestrator
//! role), and a call forwarded to another host — while an ordinary agent cannot grant itself the
//! role, a git tool may not write to another orchestrator (its worktree is the project's home
//! worktree), and a session an orchestrator starts carries the link before its agent runs
//! (`#orchestrators` 1/11).
//!
//! Cross-host calls go through the in-memory peer forwarder of `support`, never LiveKit.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-10-orchestrator-sessions.md
//! Changeset: docs/dev/1-WIP/2026-11-10-orchestrator-role.md

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use support::{
    a_host, a_host_running_claude_as, a_project, a_repo_with_origin, a_workspace_in,
    current_os_user, git_stdout, session_started_by, Host, InProcessPeers, HOST_A, HOST_B,
    THIS_PROJECT,
};
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_projects::project_storage::write_projects;
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::exec_tools::{
    ExecToolService, ExecuteToolRequest, ExecuteToolResponse, ListExecToolsRequest,
};
use tddy_service::proto::session::{SessionService as SessionServiceTrait, StartSessionRequest};
use tddy_session_lifecycle::test_util::{signing_test_host_as, SignedTestCredentials, TEST_TOKEN};
use tddy_testing_commons::wait::{a_capture_showing, PTY_STUB_OUTPUT};

const ORCHESTRATOR_ROLE: &str = "orchestrator";
const PROJECT_HOME_ROLE: &str = "project-home";
const TEST_MODEL: &str = "claude-opus-4-8";

// ── the world ───────────────────────────────────────────────────────────────────────────────

/// A stub `claude` that prints the `orchestrator_session_id` line of its session's changeset as it
/// was at the moment the agent was spawned, then stays alive like an agent waiting for input. It
/// finds its session dir from the `--session-id` it was launched with, under the sessions base the
/// test writes beside it once the host exists.
struct AClaudeStub {
    dir: tempfile::TempDir,
}

impl AClaudeStub {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("claude");
        std::fs::write(
            &script,
            [
                "#!/bin/sh\n",
                "sid=\"\"; prev=\"\"\n",
                "for a in \"$@\"; do if [ \"$prev\" = \"--session-id\" ]; then sid=\"$a\"; fi; prev=\"$a\"; done\n",
                "base=$(cat \"$(dirname \"$0\")/sessions_base\")\n",
                "line=$(grep '^orchestrator_session_id:' \"$base/sessions/$sid/changeset.yaml\" 2>/dev/null)\n",
                "echo \"AT SPAWN [$line]\"\n",
                "exec cat >/dev/null\n",
            ]
            .concat(),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir }
    }

    fn binary(&self) -> PathBuf {
        self.dir.path().join("claude")
    }

    fn serving_sessions_under(&self, sessions_base: &Path) {
        std::fs::write(
            self.dir.path().join("sessions_base"),
            sessions_base.to_string_lossy().as_bytes(),
        )
        .unwrap();
    }
}

/// One host serving a project with a main and an orchestrator, whose `claude` is [`AClaudeStub`].
struct World {
    a: Host,
    main: String,
    orchestrator: String,
    _claude: AClaudeStub,
}

fn with_role(host: &Host, session_id: &str, role: &str) {
    let dir =
        tddy_core::session_lifecycle::unified_session_dir_path(host.sessions.path(), session_id);
    let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
    meta.session_role = Some(role.to_string());
    tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();
}

async fn a_project_with_main_and_an_orchestrator() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let claude = AClaudeStub::new();
    let a = a_host_running_claude_as(HOST_A, &peers, &claude.binary());
    claude.serving_sessions_under(a.sessions.path());
    let main = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&main);
    let orchestrator = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    with_role(&a, &orchestrator, ORCHESTRATOR_ROLE);
    World {
        a,
        main,
        orchestrator,
        _claude: claude,
    }
}

async fn calls_on(
    daemon: &TestDaemon,
    session_token: &str,
    session_id: &str,
    tool: &str,
    args: serde_json::Value,
) -> Result<ExecuteToolResponse, Status> {
    daemon
        .execute_tool(Request::direct(ExecuteToolRequest {
            session_token: session_token.to_string(),
            session_id: session_id.to_string(),
            tool_name: tool.to_string(),
            args_json: args.to_string(),
            daemon_instance_id: String::new(),
            conversation_id: String::new(),
        }))
        .await
        .map(|r| r.into_inner())
}

impl World {
    async fn calls(
        &self,
        session_id: &str,
        tool: &str,
        args: serde_json::Value,
    ) -> Result<ExecuteToolResponse, Status> {
        calls_on(&self.a.daemon, TEST_TOKEN, session_id, tool, args).await
    }

    async fn offered_to(&self, session_id: &str) -> Vec<String> {
        let mut names: Vec<String> = self
            .a
            .daemon
            .list_exec_tools(Request::direct(ListExecToolsRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
                ..Default::default()
            }))
            .await
            .expect("the session's tools are listed")
            .into_inner()
            .tools
            .into_iter()
            .map(|tool| tool.name)
            .collect();
        names.sort();
        names
    }

    /// The `orchestrator_session_id` line `session_id`'s changeset held when its agent was spawned,
    /// as the stub `claude` printed it; empty when the changeset had none.
    async fn orchestrator_link_seen_at_spawn(&self, session_id: &str) -> String {
        let handle = self
            .a
            .terminals
            .get(session_id)
            .await
            .expect("the session's agent was spawned");
        let shown = a_capture_showing(&handle.capture, "AT SPAWN [", PTY_STUB_OUTPUT).await;
        let start = shown.find("AT SPAWN [").unwrap() + "AT SPAWN [".len();
        let rest = &shown[start..];
        rest[..rest.find(']').expect("the printed line is closed")].replace(['\'', '"'], "")
    }
}

/// The start a `StartSession` answer began, to its end: `Ok` with the session it started, `Err`
/// with why it was refused — whether the tool refused it outright or the start failed.
async fn the_start_outcome(
    daemon: &TestDaemon,
    session_token: &str,
    caller: &str,
    answer: &ExecuteToolResponse,
) -> Result<String, String> {
    if answer.is_error {
        return Err(answer.error_message.clone());
    }
    let handle: serde_json::Value = serde_json::from_str(&answer.result_json).expect("JSON");
    let start_id = handle["start_id"].as_str().expect("a start_id").to_string();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        let read = calls_on(
            daemon,
            session_token,
            caller,
            "GetSessionStart",
            serde_json::json!({ "start_id": start_id }),
        )
        .await
        .expect("GetSessionStart is served to the caller");
        let record: serde_json::Value = serde_json::from_str(&read.result_json).expect("JSON");
        match record["state"].as_str() {
            Some("running") => {}
            Some("succeeded") => return Ok(record["session_id"].as_str().unwrap().to_string()),
            _ => return Err(record["error"].as_str().unwrap_or_default().to_string()),
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "start {start_id} is still running"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// ── main's reach ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn an_orchestrator_is_offered_mains_tool_set() {
    // Given
    let world = a_project_with_main_and_an_orchestrator().await;

    // When
    let offered = world.offered_to(&world.orchestrator).await;

    // Then exactly what main is offered
    assert_eq!(offered, world.offered_to(&world.main).await);
}

#[tokio::test]
async fn an_orchestrator_is_refused_a_write_tool_as_main_is() {
    // Given an orchestrator over a worktree holding one file
    let world = a_project_with_main_and_an_orchestrator().await;
    let worktree = world.a.worktree_of(&world.orchestrator);
    std::fs::write(worktree.join("kept.txt"), "original\n").unwrap();

    // When it writes, edits and runs a shell
    let attempts = [
        (
            "Write",
            serde_json::json!({ "path": "new.txt", "contents": "x" }),
        ),
        (
            "StrReplace",
            serde_json::json!({ "path": "kept.txt", "old_string": "original", "new_string": "changed" }),
        ),
        (
            "Shell",
            serde_json::json!({ "command": "touch shelled.txt" }),
        ),
    ];
    let mut codes = Vec::new();
    for (tool, args) in attempts {
        let outcome = world.calls(&world.orchestrator, tool, args).await;
        codes.push((tool, outcome.map(|_| ()).map_err(|s| s.code())));
    }

    // Then each is refused, and the worktree is untouched
    assert_eq!(
        codes,
        vec![
            ("Write", Err(Code::PermissionDenied)),
            ("StrReplace", Err(Code::PermissionDenied)),
            ("Shell", Err(Code::PermissionDenied)),
        ]
    );
    assert_eq!(
        std::fs::read_to_string(worktree.join("kept.txt")).unwrap(),
        "original\n"
    );
    assert!(!worktree.join("new.txt").exists());
    assert!(!worktree.join("shelled.txt").exists());
}

// ── granting the role ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn main_starts_an_orchestrator_through_start_session() {
    // Given main on a daemon that verifies tokens, whose agents run the stub `claude`
    let world = a_main_on_a_signing_daemon().await;

    // When main asks StartSession for a Claude CLI session in the orchestrator role
    let answer = calls_on(
        &world.daemon,
        &world.credentials.user_token,
        &world.spawner,
        "StartSession",
        serde_json::json!({
            "session_type": "claude-cli",
            "model": TEST_MODEL,
            "session_role": ORCHESTRATOR_ROLE,
        }),
    )
    .await
    .expect("StartSession is served to main");
    assert!(!answer.is_error, "{}", answer.error_message);
    let started = the_start_outcome(
        &world.daemon,
        &world.credentials.user_token,
        &world.spawner,
        &answer,
    )
    .await
    .expect("the start succeeds");

    // Then the session it started holds the role, managed
    let recorded = tddy_core::read_session_metadata(
        &tddy_core::session_lifecycle::unified_session_dir_path(world.sessions.path(), &started),
    )
    .expect("the session has metadata");
    assert_eq!(
        (recorded.session_role.as_deref(), recorded.managed_codebase),
        (Some(ORCHESTRATOR_ROLE), true)
    );
}

/// A world on a daemon that verifies real tokens, so an agent's call carries the agent principal:
/// a conversation spawner — a grill-me session, offered `StartSession` but holding no orchestrator
/// role — and the credentials its agent and its person hold.
struct ASigningWorld {
    daemon: TestDaemon,
    spawner: String,
    credentials: SignedTestCredentials,
    sessions: tempfile::TempDir,
    _repo: tempfile::TempDir,
    _claude: AClaudeStub,
}

async fn a_conversation_spawner_on_a_signing_daemon() -> ASigningWorld {
    let world = a_workspace_on_a_signing_daemon().await;
    let dir = tddy_core::session_lifecycle::unified_session_dir_path(
        world.sessions.path(),
        &world.spawner,
    );
    let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
    meta.recipe = Some("grill-me".to_string());
    tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();
    world
}

/// [`a_workspace_on_a_signing_daemon`], whose workspace is the project's main.
async fn a_main_on_a_signing_daemon() -> ASigningWorld {
    let world = a_workspace_on_a_signing_daemon().await;
    let dir = tddy_core::session_lifecycle::unified_session_dir_path(
        world.sessions.path(),
        &world.spawner,
    );
    let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
    meta.session_role = Some(PROJECT_HOME_ROLE.to_string());
    tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();
    world
}

/// One workspace session (`spawner`) on a daemon that verifies tokens and starts `claude` as the
/// suite's stub.
async fn a_workspace_on_a_signing_daemon() -> ASigningWorld {
    let sessions = tempfile::tempdir().unwrap();
    let repo = a_repo_with_origin();
    write_projects(
        &sessions.path().join("projects"),
        &[a_project(THIS_PROJECT, repo.path())],
    )
    .unwrap();
    let claude = AClaudeStub::new();
    claude.serving_sessions_under(sessions.path());
    let (host, credentials) =
        signing_test_host_as(sessions.path().to_path_buf(), &current_os_user());
    let host = host.with_coding_agents(
        tddy_session_lifecycle::test_util::coding_agents_with_binaries(&[(
            "claude",
            &claude.binary(),
        )]),
    );
    let daemon = TestDaemon::from_host(host);
    daemon.as_arc().install_sandbox_rpc_bridge();
    let spawner = daemon
        .start_session(Request::direct(StartSessionRequest {
            session_token: credentials.user_token.clone(),
            project_id: THIS_PROJECT.to_string(),
            session_type: "workspace".to_string(),
            ..Default::default()
        }))
        .await
        .expect("the spawner's session starts")
        .into_inner()
        .session_id;
    ASigningWorld {
        daemon,
        spawner,
        credentials,
        sessions,
        _repo: repo,
        _claude: claude,
    }
}

impl ASigningWorld {
    /// Every session on the daemon that holds the orchestrator role.
    fn orchestrators(&self) -> Vec<String> {
        let sessions = self.sessions.path().join("sessions");
        std::fs::read_dir(&sessions)
            .map(|dirs| {
                dirs.filter_map(Result::ok)
                    .filter(|dir| {
                        tddy_core::read_session_metadata(&dir.path()).is_ok_and(|meta| {
                            meta.session_role.as_deref() == Some(ORCHESTRATOR_ROLE)
                        })
                    })
                    .map(|dir| dir.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[tokio::test]
async fn an_ordinary_agent_asking_start_session_for_the_orchestrator_role_is_refused() {
    // Given a conversation spawner's agent, which may start sessions but holds no orchestrator role
    let world = a_conversation_spawner_on_a_signing_daemon().await;

    // When its agent asks StartSession for an orchestrator, with the token minted for it
    let answer = calls_on(
        &world.daemon,
        &world.credentials.agent_token,
        &world.spawner,
        "StartSession",
        serde_json::json!({
            "session_type": "claude-cli",
            "model": TEST_MODEL,
            "session_role": ORCHESTRATOR_ROLE,
        }),
    )
    .await
    .expect("StartSession is served to a conversation spawner");
    let outcome = the_start_outcome(
        &world.daemon,
        &world.credentials.agent_token,
        &world.spawner,
        &answer,
    )
    .await;

    // Then the start is refused for the role it asked for, and no orchestrator exists
    let refusal = outcome.expect_err("an agent without an orchestrator role cannot grant it");
    assert!(
        refusal.contains(ORCHESTRATOR_ROLE),
        "names the refused role: {refusal}"
    );
    assert_eq!(world.orchestrators(), Vec::<String>::new());
}

#[tokio::test]
async fn a_session_started_by_an_orchestrator_records_it_before_its_agent_runs() {
    // Given
    let world = a_project_with_main_and_an_orchestrator().await;

    // When the orchestrator starts a Claude CLI session
    let answer = world
        .calls(
            &world.orchestrator,
            "StartSession",
            serde_json::json!({
                "session_type": "claude-cli",
                "model": TEST_MODEL,
                "branch_worktree_intent": "new_branch_from_base",
                "new_branch_name": "orchestrated/child",
            }),
        )
        .await
        .expect("StartSession is served to an orchestrator");
    assert!(!answer.is_error, "{}", answer.error_message);
    let child = session_started_by(&world.a.daemon, &world.orchestrator, &answer.result_json).await;

    // Then the child's changeset named the orchestrator already when its agent was spawned
    assert_eq!(
        world.orchestrator_link_seen_at_spawn(&child).await,
        format!("orchestrator_session_id: {}", world.orchestrator)
    );
}

// ── the shared home worktree ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_git_tool_targeting_another_orchestrator_is_refused() {
    // Given an orchestrator with an uncommitted file in its worktree
    let world = a_project_with_main_and_an_orchestrator().await;
    let worktree = world.a.worktree_of(&world.orchestrator);
    std::fs::write(worktree.join("theirs.txt"), "not main's to commit\n").unwrap();

    // When main commits in it
    let commit = world
        .calls(
            &world.main,
            "CommitSessionWorktree",
            serde_json::json!({ "session_id": world.orchestrator, "message": "take it" }),
        )
        .await
        .expect("CommitSessionWorktree is served to main");

    // Then it is refused, naming why, and nothing was committed
    assert!(
        commit.is_error,
        "a write to another orchestrator is refused"
    );
    assert!(
        commit.error_message.contains(ORCHESTRATOR_ROLE)
            && commit.error_message.contains(&world.orchestrator),
        "names the orchestrator: {}",
        commit.error_message
    );
    assert_eq!(
        git_stdout(&worktree, &["status", "--porcelain", "--", "theirs.txt"]),
        "?? theirs.txt"
    );
}

// ── across hosts ────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_forwarded_call_from_an_orchestrator_is_served_by_the_other_host() {
    // Given an orchestrator on host-a, and a file in the worktree of the project's session on host-b
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let b = a_host(HOST_B, &peers);
    let orchestrator = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    with_role(&a, &orchestrator, ORCHESTRATOR_ROLE);
    let on_b = a_workspace_in(&b.daemon, THIS_PROJECT).await;
    std::fs::write(b.worktree_of(&on_b).join("notes.md"), "on host b\n").unwrap();

    // When the orchestrator reads it, naming host-b
    let read = calls_on(
        &a.daemon,
        TEST_TOKEN,
        &orchestrator,
        "ReadSessionWorktree",
        serde_json::json!({ "session_id": on_b, "path": "notes.md", "daemon_instance_id": HOST_B }),
    )
    .await
    .expect("ReadSessionWorktree is served to an orchestrator");

    // Then host-b served it, having confirmed the caller's role on host-a
    assert!(!read.is_error, "{}", read.error_message);
    let content: serde_json::Value = serde_json::from_str(&read.result_json).expect("JSON");
    assert_eq!(content["content"], "on host b\n");
}
