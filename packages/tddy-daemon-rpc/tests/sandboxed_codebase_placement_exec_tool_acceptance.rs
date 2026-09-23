//! Acceptance tests: the **sandboxed codebase** placement — this daemon jails its own checkout in
//! a `--workspace-tools` jail and runs the agent beside it, unconfined, with every native
//! filesystem and shell tool withdrawn.
//!
//! It is the third placement, not a shade of the other two. `CoLocated` and `Split` differ only in
//! *which host* holds the checkout; both leave the jail question to `sandbox`, which jails the
//! **agent**. This one inverts that: the code is confined and the agent is not.
//!
//! Implemented as the split orchestration with the peer hop removed — a **local** `workspace`
//! session with `sandbox: Some(true)` holds the worktree, and the agent's `mcp__tddy-tools__*`
//! calls address *that* session, so `exec_tool_route`'s existing predicate
//! (`session_type == "workspace" && sandbox == Some(true)`) lands them in the jail unchanged.
//!
//! These tests need **no common room**, and one of them asserts exactly that: a placement whose
//! whole point is that both halves are on this host must not require one. The `livekit:` block in
//! the fixture config carries `api_secret` and nothing else — the deployment's session-token
//! signer, not a room: `livekit.enabled` defaults to false and no `common_room` is named.
//!
//! Everything here is asserted through the RPC surface or through what the daemon persisted —
//! `StartSession`, `ExecuteTool`, and `.session.yaml`. The agent's argv and its `TDDY_REMOTE_*`
//! environment are `pub(crate)` details and are pinned by unit tests inside
//! `tddy-session-lifecycle`, not by a test-only `pub` opened for this suite.
//!
//! Subject crate: `tddy-session-lifecycle` (the placement and the start path), with
//! `tddy-daemon-sandbox` consumed unchanged. Filed here beside the existing sandbox suites — see
//! `packages/tddy-daemon/docs/code-issues/misplaced-tests-integration-suites.md` § Concurrent
//! changes, which routes it.
//!
//! PRD: docs/ft/daemon/amendments/PRD-2026-09-18-sandboxed-codebase-from-the-web.md
//! Changeset: docs/dev/1-WIP/2026-09-18-sandboxed-codebase-mode-from-the-web.md
//!
//! The exec-tool half of `tddy-session-lifecycle`'s `sandboxed_codebase_placement_acceptance.rs`,
//! split out because the exec tools are served by `tddy-daemon-rpc`'s `ExecToolRpcHandler`. The
//! fixture is that suite's, trimmed to what these tests reach.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::{read_session_metadata, SessionMetadata};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_livekit::livekit_peer_discovery::LiveKitDiscoveryHandles;
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_github::{GitHubUser, SessionTokenSigner};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest};
use tddy_service::proto::session::{SessionService as SessionServiceTrait, StartSessionRequest};
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const LOCAL_INSTANCE_ID: &str = "workstation";
/// The deployment secret this daemon signs its session tokens with. It is what lets the daemon
/// mint the agent's own credential for the tool calls it makes back here — without it the start
/// is refused rather than forwarding the caller's token onward, so a fixture without one would
/// exercise a refusal instead of the placement.
const LK_API_SECRET: &str = "secret";
/// A specialized agent this deployment offers, as the create-session form would name it.
const A_SPECIALIZED_AGENT: &str = "fastcontext";
const PROJECT_ID: &str = "019d105b-ac0f-78d3-9a89-409731145a36";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// The credential the browser presents, signed with [`LK_API_SECRET`] so this daemon can verify
/// it and mint the agent's own from the identity it proves. Minted once and shared, because the
/// request and the daemon's user resolver have to agree on the very same string.
fn a_caller_token() -> &'static str {
    static TOKEN: OnceLock<String> = OnceLock::new();
    TOKEN.get_or_init(|| {
        SessionTokenSigner::new(LK_API_SECRET.as_bytes()).mint_access(&GitHubUser {
            id: 4242,
            login: current_os_user(),
            avatar_url: "https://avatars.githubusercontent.com/u/4242?v=4".to_string(),
            name: "Test User".to_string(),
        })
    })
}

/// The OS user this test process runs as — a real, resolvable one. A fabricated name does not
/// resolve during the claude-cli spawn, and these are the first sandboxed-codebase suites that
/// actually complete one. Same pattern as `claude_cli_session_acceptance.rs`.
fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

fn run_git(cwd: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .status()
        .unwrap_or_else(|e| panic!("git {args:?} failed to run: {e}"));
    assert!(status.success(), "git {args:?} must succeed in {cwd:?}");
}

/// A repository with a committed `README.md` — the file the tool calls below read. An empty repo
/// would let a jail answer "file not found" and the routing assertion would pass for the wrong
/// reason.
fn a_git_repo_with_origin_at(path: &Path) {
    // Idempotent: a restart test stands a second daemon on the same sessions base, which would
    // otherwise re-run `git commit` on a tree with nothing to commit.
    if path.join(".git").is_dir() {
        return;
    }
    std::fs::create_dir_all(path).expect("create fixture repo dir");
    run_git(path, &["init", "-q", "-b", "main"]);
    run_git(path, &["config", "user.email", "t@t.com"]);
    run_git(path, &["config", "user.name", "Test"]);
    std::fs::write(path.join("README.md"), "# fixture\n").expect("write README");
    run_git(path, &["add", "README.md"]);
    run_git(path, &["commit", "-q", "-m", "init"]);
    run_git(path, &["remote", "add", "origin", path.to_str().unwrap()]);
    run_git(path, &["push", "-q", "-u", "origin", "main"]);
}

fn register_project(sessions_base: &Path, repo_path: &Path) {
    tddy_projects::project_storage::write_projects(
        &sessions_base.join("projects"),
        &[tddy_projects::project_storage::ProjectData {
            project_id: PROJECT_ID.to_string(),
            name: "sandboxed-codebase".to_string(),
            git_url: String::new(),
            main_repo_path: repo_path.display().to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: Default::default(),
        }],
    )
    .expect("register project");
}

fn test_config() -> DaemonConfig {
    let user = current_os_user();
    let yaml = format!(
        r#"
users:
  - github_user: "{user}"
    os_user: "{user}"
daemon_instance_id: "{LOCAL_INSTANCE_ID}"
claude_cli:
  binary_path: /bin/cat
livekit:
  api_secret: "{LK_API_SECRET}"
"#
    );
    serde_yaml::from_str(&yaml).expect("config must parse")
}

fn user_resolver_valid() -> UserResolver {
    Arc::new(|token| {
        if token == a_caller_token() {
            Some(current_os_user())
        } else {
            None
        }
    })
}

/// Define one specialized agent on this daemon, under `<tddyhome>/agents`.
///
/// Part of the fixture deployment rather than of a single test: a start naming an agent no host
/// defines is refused by name (`seeded_roster_records`), so without a def the managed cases below
/// would exercise that refusal instead of the placement.
fn an_agent_def_on_this_host(tddy_data_dir: &Path, name: &str) {
    let agents = tddy_data_dir.join("agents");
    std::fs::create_dir_all(&agents).expect("create agents dir");
    std::fs::write(
        agents.join(format!("{name}.yaml")),
        format!(
            "name: {name}\nmodel: qwen2.5-coder:7b\nbase_url: http://127.0.0.1:11434/v1\nreplaces:\n  - Grep\n"
        ),
    )
    .expect("write agent def");
}

fn a_daemon(sessions_base: PathBuf, discovery: Option<LiveKitDiscoveryHandles>) -> TestDaemon {
    an_agent_def_on_this_host(&sessions_base, A_SPECIALIZED_AGENT);
    let repo = sessions_base.join("fixture-repo");
    a_git_repo_with_origin_at(&repo);
    register_project(&sessions_base, &repo);
    let resolver: SessionsBaseResolver = {
        let base = sessions_base.clone();
        Arc::new(move |_| Some(base.clone()))
    };
    TestDaemon::from_host(DaemonSessionHost::new(
        test_config(),
        resolver,
        sessions_base,
        user_resolver_valid(),
        None,
        discovery,
        None,
        Arc::new(ClaudeCliSessionManager::new()),
    ))
}

/// A daemon with **no LiveKit discovery at all** — no eligible-peer source, no common room.
///
/// This is the fixture that matters for this placement: both halves live here, so a daemon that
/// has never joined a room must still serve it. A fixture carrying discovery handles would let a
/// hidden dependency on peer discovery pass unnoticed.
///
/// Its config does carry `livekit.api_secret`, which is not a room: `livekit.enabled` defaults to
/// false and no `common_room` is named, so nothing here joins one. That secret is the deployment's
/// session-token signer (`tddy-daemon-auth`), and it is the whole reason this daemon can mint the
/// agent a credential of its own instead of refusing the start.
fn a_daemon_with_no_common_room(sessions_base: PathBuf) -> TestDaemon {
    a_daemon(sessions_base, None)
}

/// A claude-cli start request asking for the jailed-codebase placement and nothing else.
fn a_sandboxed_codebase_request() -> StartSessionRequest {
    StartSessionRequest {
        session_token: a_caller_token().to_string(),
        project_id: PROJECT_ID.to_string(),
        session_type: "claude-cli".to_string(),
        model: "claude-opus-5".to_string(),
        sandboxed_codebase: true,
        ..Default::default()
    }
}

fn a_read_of(session_id: &str) -> ExecuteToolRequest {
    ExecuteToolRequest {
        session_token: a_caller_token().to_string(),
        session_id: session_id.to_string(),
        tool_name: "Read".to_string(),
        args_json: r#"{"path":"README.md"}"#.to_string(),
        ..Default::default()
    }
}

fn metadata_of(sessions_base: &Path, session_id: &str) -> SessionMetadata {
    read_session_metadata(&unified_session_dir_path(sessions_base, session_id))
        .unwrap_or_else(|e| panic!("session {session_id} must have readable metadata: {e}"))
}

/// The workspace session holding a jailed-codebase session's checkout.
fn checkout_session_of(sessions_base: &Path, agent_session_id: &str) -> String {
    metadata_of(sessions_base, agent_session_id)
        .codebase_session_id
        .expect("the agent half must record the workspace session holding its checkout")
}

// ---------------------------------------------------------------------------
// classify_placement — which of three placements a request asks for
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// StartSession — what the placement persists
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// ExecuteTool — the code is reached through the jail, or not at all
// ---------------------------------------------------------------------------

#[cfg_attr(
    not(target_os = "macos"),
    ignore = "needs a host that permits unprivileged user namespaces: the cgroups jail cannot be \
              provisioned without them, and GitHub's ubuntu runners set \
              kernel.apparmor_restrict_unprivileged_userns=1. The daemon refuses with \
              FailedPrecondition rather than starting an unconfined session, which is correct — see \
              docs/dev/todo/2026-08-02-unprivileged-userns-available-under-approximates-what-the-jail-needs.md"
)]
#[tokio::test]
async fn a_sandboxed_codebase_sessions_tool_call_is_served_by_its_jail() {
    // Given a started jailed-codebase session
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = a_daemon_with_no_common_room(sessions_tmp.path().to_path_buf());
    let started = service
        .start_session(Request::new(a_sandboxed_codebase_request()))
        .await
        .expect("a jailed-codebase session must start")
        .into_inner();
    let checkout = checkout_session_of(sessions_tmp.path(), &started.session_id);

    // When the agent reads a file through the session its MCP addresses
    let response = service
        .execute_tool(Request::new(a_read_of(&checkout)))
        .await
        .expect("a jailed checkout must serve its own tool calls")
        .into_inner();

    // Then the call was answered by the jail, not refused for want of one
    assert!(
        !response.is_error,
        "a registered jail must serve the call; got '{}'",
        response.error_message
    );

    // Teardown. A jail is a real `tddy-sandbox-runner` child in its own process group, reparented
    // to the init process when this test binary exits, so a test that merely stops naming it
    // leaves it on the host for as long as the machine is up. `shut_down_children` is the
    // production shutdown path (the daemon's SIGTERM handler calls it), driven here so the test
    // says where its jail dies; `WorkspaceSandboxRegistry`'s own `Drop` is what covers the
    // panicking exit above, where this line is never reached.
    service.shut_down_children().await;
}

#[cfg_attr(
    not(target_os = "macos"),
    ignore = "needs a host that permits unprivileged user namespaces: the cgroups jail cannot be \
              provisioned without them, and GitHub's ubuntu runners set \
              kernel.apparmor_restrict_unprivileged_userns=1. The daemon refuses with \
              FailedPrecondition rather than starting an unconfined session, which is correct — see \
              docs/dev/todo/2026-08-02-unprivileged-userns-available-under-approximates-what-the-jail-needs.md"
)]
#[tokio::test]
async fn a_sandboxed_codebase_sessions_tool_call_is_refused_when_its_jail_is_gone() {
    // Given a jailed-codebase session whose daemon was restarted, losing the jail registration —
    // the workspace metadata still says sandboxed, and nothing holds a jail for it
    let sessions_tmp = tempfile::tempdir().unwrap();
    let started = {
        let service = a_daemon_with_no_common_room(sessions_tmp.path().to_path_buf());
        let started = service
            .start_session(Request::new(a_sandboxed_codebase_request()))
            .await
            .expect("a jailed-codebase session must start")
            .into_inner();
        // The restart this test simulates is a daemon *stopping*, so the jail it held stops with
        // it. Letting the daemon merely fall out of scope here would orphan a live
        // `tddy-sandbox-runner` onto the host — and would make the test's own premise wrong, since
        // a restarted daemon does not leave its predecessor's runner behind.
        service.shut_down_children().await;
        started
    };
    let checkout = checkout_session_of(sessions_tmp.path(), &started.session_id);
    let restarted = a_daemon_with_no_common_room(sessions_tmp.path().to_path_buf());

    // When the agent reads a file
    let response = restarted
        .execute_tool(Request::new(a_read_of(&checkout)))
        .await
        .expect("the refusal is the tool's answer, not a transport failure")
        .into_inner();

    // Then it is refused. A tool that ran unconfined on a session that asked to be confined is the
    // one failure nobody can see afterwards.
    assert!(
        response.is_error,
        "a sandboxed session with no jail must be refused, never served from the bare host"
    );
    assert!(
        response.error_message.contains("sandboxed"),
        "the refusal must say the session asked to be confined; got '{}'",
        response.error_message
    );

    // Teardown. The restarted daemon provisioned no jail — that refusal is the point — but it is
    // shut down for the same reason as every other daemon here, so no suite reads "nothing to
    // stop" as "no teardown needed".
    restarted.shut_down_children().await;
}

// ---------------------------------------------------------------------------
// StartSession — the refusals, at the request level
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// The managed workflow over a jailed codebase
// ---------------------------------------------------------------------------
