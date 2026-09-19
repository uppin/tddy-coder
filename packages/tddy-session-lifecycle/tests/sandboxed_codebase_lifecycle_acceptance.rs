//! Acceptance tests: a jailed-codebase session's jail lives and dies with the session.
//!
//! The placement reuses the split pairing (`codebase_daemon_instance_id` + `codebase_session_id`),
//! so `DeleteSession` and resume already follow it. What is *not* already true is shutdown:
//! `tddy-daemon`'s SIGTERM path calls `cli_sessions.kill_all()` and nothing else, so a
//! `tddy-sandbox-runner` outlives the daemon that spawned it
//! (docs/dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md). This
//! changeset closes that for the workspace-jail registry; the crash-detector and restart-policy
//! half of that entry stays open.
//!
//! A jail is a real child process, so these are integration tests: liveness is read from the pid
//! the jail recorded (`workspace_tool_sandbox::RUNNER_PID_FILE`), not from a mock's opinion.
//!
//! Subject crate: `tddy-session-lifecycle`, plus `tddy-daemon`'s own shutdown wiring — which is
//! composition-root code and genuinely belongs to this crate.
//!
//! PRD: docs/ft/daemon/amendments/PRD-2026-09-18-sandboxed-codebase-from-the-web.md
//! Changeset: docs/dev/1-WIP/2026-09-18-sandboxed-codebase-mode-from-the-web.md

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::{read_session_metadata, SessionMetadata};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_sandbox::workspace_tool_sandbox::RUNNER_PID_FILE;
use tddy_github::{GitHubUser, SessionTokenSigner};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest};
use tddy_service::proto::session::{
    DeleteSessionRequest, ResumeSessionRequest, SessionService as SessionServiceTrait,
    StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::test_util::TestDaemon;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const LOCAL_INSTANCE_ID: &str = "workstation";
/// The deployment secret this daemon signs its session tokens with — what lets it mint the agent
/// a credential of its own for the tool calls it makes back here. Without it the start is refused
/// rather than forwarding the caller's token, so every test below would exercise that refusal.
const LK_API_SECRET: &str = "secret";
const PROJECT_ID: &str = "019d105b-ac0f-78d3-9a89-409731145a36";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// The credential the browser presents, signed with [`LK_API_SECRET`] so this daemon can verify it
/// and mint the agent's own from the identity it proves. Minted once and shared, because the
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

/// A daemon with no peer discovery and no common room.
///
/// Its config does carry `livekit.api_secret`, which is not a room: `livekit.enabled` defaults to
/// false and no `common_room` is named. That secret is the deployment's session-token signer, and
/// it is what lets this daemon mint the agent's own credential instead of refusing the start.
fn a_daemon_with_no_common_room(sessions_base: PathBuf) -> TestDaemon {
    let repo = sessions_base.join("fixture-repo");
    a_git_repo_with_origin_at(&repo);
    register_project(&sessions_base, &repo);
    let resolver: SessionsBaseResolver = {
        let base = sessions_base.clone();
        Arc::new(move |_| Some(base.clone()))
    };
    TestDaemon::from_arc(Arc::new(DaemonSessionHost::new(
        test_config(),
        resolver,
        sessions_base,
        user_resolver_valid(),
        None,
        None,
        None,
        Arc::new(ClaudeCliSessionManager::new()),
    )))
}

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

fn metadata_of(sessions_base: &Path, session_id: &str) -> SessionMetadata {
    read_session_metadata(&unified_session_dir_path(sessions_base, session_id))
        .unwrap_or_else(|e| panic!("session {session_id} must have readable metadata: {e}"))
}

fn checkout_session_of(sessions_base: &Path, agent_session_id: &str) -> String {
    metadata_of(sessions_base, agent_session_id)
        .codebase_session_id
        .expect("the agent half must record the workspace session holding its checkout")
}

/// The pid the jail recorded for its `tddy-sandbox-runner`, from the file the provisioner writes.
fn recorded_runner_pid(sessions_base: &Path, checkout_session_id: &str) -> u32 {
    let pid_file = unified_session_dir_path(sessions_base, checkout_session_id)
        .join("sandbox")
        .join(RUNNER_PID_FILE);
    let raw = std::fs::read_to_string(&pid_file)
        .unwrap_or_else(|e| panic!("the jail must record its runner pid at {pid_file:?}: {e}"));
    raw.trim()
        .parse()
        .unwrap_or_else(|e| panic!("{pid_file:?} must hold a pid, got {raw:?}: {e}"))
}

/// Whether a process is still there, by the signal-0 liveness probe.
fn process_is_alive(pid: u32) -> bool {
    // SAFETY: `kill(pid, 0)` performs the permission and existence check without delivering a
    // signal — the standard liveness probe, and the same one `terminate_sandbox_process` relies on.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

fn a_deletion_of(session_id: &str) -> DeleteSessionRequest {
    DeleteSessionRequest {
        session_token: a_caller_token().to_string(),
        session_id: session_id.to_string(),
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

// ---------------------------------------------------------------------------
// Delete
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
async fn deleting_a_sandboxed_codebase_session_tears_its_jail_down() {
    // Given a started jailed-codebase session with a live runner
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = a_daemon_with_no_common_room(sessions_tmp.path().to_path_buf());
    let started = service
        .start_session(Request::new(a_sandboxed_codebase_request()))
        .await
        .expect("a jailed-codebase session must start")
        .into_inner();
    let checkout = checkout_session_of(sessions_tmp.path(), &started.session_id);
    let runner = recorded_runner_pid(sessions_tmp.path(), &checkout);
    assert!(
        process_is_alive(runner),
        "the jail must be running to start"
    );

    // When the operator deletes the session
    service
        .delete_session(Request::new(a_deletion_of(&started.session_id)))
        .await
        .expect("deleting a jailed-codebase session must succeed");

    // Then the jail's runner is gone with it
    assert!(
        !process_is_alive(runner),
        "the jail must not outlive the session that asked for it"
    );
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
async fn deleting_a_sandboxed_codebase_session_removes_its_paired_checkout_session() {
    // Given a started jailed-codebase session
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = a_daemon_with_no_common_room(sessions_tmp.path().to_path_buf());
    let started = service
        .start_session(Request::new(a_sandboxed_codebase_request()))
        .await
        .expect("a jailed-codebase session must start")
        .into_inner();
    let checkout = checkout_session_of(sessions_tmp.path(), &started.session_id);

    // When the operator deletes the agent half
    service
        .delete_session(Request::new(a_deletion_of(&started.session_id)))
        .await
        .expect("deleting a jailed-codebase session must succeed");

    // Then the workspace half goes too — a checkout session with no agent is a stranded worktree
    assert!(
        !unified_session_dir_path(sessions_tmp.path(), &checkout).exists(),
        "the paired checkout session must be removed with the agent that owned it"
    );
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
async fn deleting_a_sandboxed_codebase_session_whose_checkout_is_already_gone_succeeds() {
    // Given a jailed-codebase session whose paired checkout was deleted on its own — an operator
    // removing it directly, or an earlier deletion that got that far and then failed on this half
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = a_daemon_with_no_common_room(sessions_tmp.path().to_path_buf());
    let started = service
        .start_session(Request::new(a_sandboxed_codebase_request()))
        .await
        .expect("a jailed-codebase session must start")
        .into_inner();
    let checkout = checkout_session_of(sessions_tmp.path(), &started.session_id);
    service
        .delete_session(Request::new(a_deletion_of(&checkout)))
        .await
        .expect("the checkout session must be deletable on its own");

    // When the operator deletes the agent half
    let deleted = service
        .delete_session(Request::new(a_deletion_of(&started.session_id)))
        .await;

    // Then it succeeds. A checkout that is provably gone is the state this teardown exists to
    // reach, not a failure to reach it — refusing on it would leave the agent half permanently
    // undeletable, because every retry would re-ask for a session that is not there.
    assert!(
        deleted.is_ok(),
        "a paired checkout that is already gone must be idempotent, not a permanent refusal: {:?}",
        deleted.err().map(|e| e.message().to_string())
    );
    assert!(
        !unified_session_dir_path(sessions_tmp.path(), &started.session_id).exists(),
        "the agent half must actually be removed, not merely reported as deleted"
    );

    // Teardown. Both deletions above are the subject of this test rather than its cleanup, and a
    // deletion that did not reach the jail is exactly the failure it could be hiding — so the
    // daemon is shut down explicitly rather than trusted to have nothing left.
    service.shut_down_children().await;
}

// ---------------------------------------------------------------------------
// Resume
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
async fn resuming_a_sandboxed_codebase_session_re_provisions_its_jail() {
    // Given a jailed-codebase session whose daemon restarted, losing every jail registration
    let sessions_tmp = tempfile::tempdir().unwrap();
    let started = {
        let service = a_daemon_with_no_common_room(sessions_tmp.path().to_path_buf());
        let started = service
            .start_session(Request::new(a_sandboxed_codebase_request()))
            .await
            .expect("a jailed-codebase session must start")
            .into_inner();
        // The restart this test simulates is a daemon *stopping*: its jail stops with it. Letting
        // the daemon merely fall out of scope here would orphan a live `tddy-sandbox-runner`, and
        // would leave the resumed daemon re-provisioning a jail beside a still-running one — which
        // is not the state the resume path is supposed to be resuming from.
        let first_runner = recorded_runner_pid(
            sessions_tmp.path(),
            &checkout_session_of(sessions_tmp.path(), &started.session_id),
        );
        service.shut_down_children().await;
        assert!(
            !process_is_alive(first_runner),
            "the pre-restart jail must be gone before the resume, or this test resumes onto a \
             host that never lost it"
        );
        started
    };
    let checkout = checkout_session_of(sessions_tmp.path(), &started.session_id);
    let restarted = a_daemon_with_no_common_room(sessions_tmp.path().to_path_buf());

    // When the session is resumed
    restarted
        .resume_session(Request::new(ResumeSessionRequest {
            session_token: a_caller_token().to_string(),
            session_id: started.session_id.clone(),
        }))
        .await
        .expect("a jailed-codebase session must be resumable");

    // Then its tool calls are served again — the jail was re-provisioned from persisted metadata,
    // not silently redirected to the bare host worktree
    let response = restarted
        .execute_tool(Request::new(a_read_of(&checkout)))
        .await
        .expect("a resumed jailed checkout must serve its tool calls")
        .into_inner();
    assert!(
        !response.is_error,
        "resume must re-provision the jail; got '{}'",
        response.error_message
    );

    // Teardown. The resume provisioned a *second* jail, which is a real `tddy-sandbox-runner`
    // child in its own process group: left alone it is reparented to the init process when this
    // binary exits and stays on the host for as long as the machine is up.
    // `WorkspaceSandboxRegistry`'s own `Drop` covers the panicking exit above, where this line is
    // never reached.
    restarted.shut_down_children().await;
}

// ---------------------------------------------------------------------------
// Shutdown — docs/dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md
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
async fn a_daemon_shutdown_leaves_no_sandbox_runner_behind() {
    // Given a started jailed-codebase session with a live runner
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = a_daemon_with_no_common_room(sessions_tmp.path().to_path_buf());
    let started = service
        .start_session(Request::new(a_sandboxed_codebase_request()))
        .await
        .expect("a jailed-codebase session must start")
        .into_inner();
    let checkout = checkout_session_of(sessions_tmp.path(), &started.session_id);
    let runner = recorded_runner_pid(sessions_tmp.path(), &checkout);
    assert!(
        process_is_alive(runner),
        "the jail must be running to start"
    );

    // When the daemon reaps its children on shutdown, without anyone deleting the session first.
    //
    // `shut_down_children` is the named seam this changeset adds: today the SIGTERM body is inline
    // in `tddy-daemon/src/main.rs:153-173` and reaches only `cli_sessions.kill_all()` plus the
    // index daemon, so nothing outside that closure can drive it — and nothing reaches the
    // workspace-jail registry at all. The handler calls this method; so does this test.
    service.shut_down_children().await;

    // Then the runner it spawned went with it, rather than being orphaned onto the host
    assert!(
        !process_is_alive(runner),
        "a jail must not survive the daemon that spawned it"
    );
}
