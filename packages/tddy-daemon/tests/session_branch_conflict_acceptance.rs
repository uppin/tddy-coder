//! Acceptance tests: `StartSession` refuses to create a session when the requested new branch is
//! already owned by another session, instead of silently creating a `<branch>-1` suffixed branch.
//!
//! The refusal is reported as `StartSessionResponse.branch_conflict` with an empty `session_id` —
//! a populated response field rather than an RPC error, because `tddy_rpc::Status` carries no
//! details and `StartSession` is forwarded between hosts over LiveKit.
//!
//! Only a *session-owned* branch conflicts. A branch that merely exists in git keeps the suffixing
//! behaviour: there is no session to switch to and no second agent to add.
//!
//! PRD: docs/ft/daemon/session-branch-conflict.md

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_core::changeset::{read_changeset, Changeset};
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_daemon::cli_session_manager::CliSessionManager;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::{
    ConnectionServiceImpl, SessionUserResolver, SessionsBaseResolver,
};
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, StartSessionRequest, StartSessionResponse,
};
use tddy_testing_commons::{a_session_metadata, fs::write_session_yaml};

const VALID_TOKEN: &str = "valid-token";
const TEST_MODEL: &str = "claude-opus-4-8";
const TEST_PROJECT_ID: &str = "test-project";

/// The branch the operator asks for, owned by `OWNER_SESSION` in most tests below.
const OWNED_BRANCH: &str = "feat/auth";
const OWNER_SESSION: &str = "019d6392-3cff-0001-aaaa-000000000001";
const SECOND_OWNER_SESSION: &str = "019d6392-3cff-0001-aaaa-000000000002";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A worktree path spelled the way the daemon records it on the changeset: it resolves a worktree
/// before writing it, so on macOS its answer is under `/private/tmp` where the fixture's `TempDir`
/// says `/tmp` — the same directory reached through a symlink the daemon has already followed.
fn as_the_daemon_records_it(worktree: &Path) -> PathBuf {
    std::fs::canonicalize(worktree).expect("the worktree must exist before it can be resolved")
}

/// The OS user the test process runs as — the config must map a real user because a *successful*
/// start_session spawns the CLI as that user.
fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

/// `/bin/cat` stands in for the `claude` binary — it runs in a PTY without the real thing.
fn a_config() -> (tempfile::TempDir, DaemonConfig) {
    let dir = tempfile::tempdir().unwrap();
    let user = current_os_user();
    let yaml = format!(
        r#"
users:
  - github_user: "{user}"
    os_user: "{user}"
allowed_tools:
  - path: /bin/true
    label: true
claude_cli:
  binary_path: /bin/cat
"#
    );
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    let config = DaemonConfig::load(&config_path).expect("config must parse");
    (dir, config)
}

fn a_service(config: DaemonConfig, sessions_base: PathBuf) -> ConnectionServiceImpl {
    let tddy_data_dir = sessions_base.clone();
    let base = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
    let resolved_user = current_os_user();
    let user_resolver: SessionUserResolver =
        Arc::new(move |token| (token == VALID_TOKEN).then(|| resolved_user.clone()));
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

fn git(repo: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .output()
        .expect("git must run");
    assert!(
        out.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A git repo whose `origin` is itself, so `git fetch origin` works with no server.
fn a_repo(dir: &Path) {
    git(dir, &["init", "-b", "main"]);
    git(dir, &["config", "user.email", "t@t.com"]);
    git(dir, &["config", "user.name", "Test"]);
    git(dir, &["commit", "--allow-empty", "-m", "init"]);
    git(dir, &["remote", "add", "origin", dir.to_str().unwrap()]);
    git(dir, &["push", "-u", "origin", "main"]);
}

fn register_project(projects_dir: &Path, repo_path: &Path) {
    std::fs::create_dir_all(projects_dir).unwrap();
    let yaml = format!(
        "projects:\n  - project_id: {}\n    name: test-project\n    git_url: \"\"\n    main_repo_path: {}\n",
        TEST_PROJECT_ID,
        repo_path.to_str().unwrap()
    );
    std::fs::write(projects_dir.join("projects.yaml"), yaml).unwrap();
}

/// A session that owns `branch`. `alive` makes it report `is_active` (a live pid);
/// `updated_at` drives the tie-break between two idle owners.
fn a_session_owning(
    sessions_base: &Path,
    session_id: &str,
    branch: &str,
    alive: bool,
    updated_at: &str,
) {
    let dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
    std::fs::create_dir_all(&dir).unwrap();
    tddy_core::write_changeset(
        &dir,
        &Changeset {
            branch: Some(branch.to_string()),
            ..Changeset::default()
        },
    )
    .unwrap();
    let builder = a_session_metadata()
        .with_session_id(session_id)
        .with_status(if alive { "active" } else { "idle" });
    // The current process is by definition alive, so it is the only pid guaranteed to make
    // `is_active` true; a session with no pid reads as idle.
    let mut meta = if alive {
        builder.with_pid(std::process::id()).build()
    } else {
        builder.build()
    };
    meta.updated_at = updated_at.to_string();
    write_session_yaml(&dir, &meta);
}

/// How many session directories exist — a refusal must not add one.
fn session_dir_count(sessions_base: &Path) -> usize {
    let root = sessions_base.join(SESSIONS_SUBDIR);
    match std::fs::read_dir(&root) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .count(),
        Err(_) => 0,
    }
}

fn local_branches(repo: &Path) -> Vec<String> {
    let out = std::process::Command::new("git")
        .args(["for-each-ref", "--format=%(refname:short)", "refs/heads"])
        .current_dir(repo)
        .output()
        .expect("git for-each-ref must run");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn a_new_branch_request(branch: &str, on_branch_conflict: &str) -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: "claude-cli".to_string(),
        model: TEST_MODEL.to_string(),
        branch_worktree_intent: "new_branch_from_base".to_string(),
        new_branch_name: branch.to_string(),
        on_branch_conflict: on_branch_conflict.to_string(),
        ..Default::default()
    }
}

/// A repo + project + service, plus the sessions root they share.
struct World {
    _repo_dir: tempfile::TempDir,
    _config_dir: tempfile::TempDir,
    sessions_tmp: tempfile::TempDir,
    repo: PathBuf,
    service: ConnectionServiceImpl,
}

fn a_world() -> World {
    let repo_dir = tempfile::tempdir().unwrap();
    a_repo(repo_dir.path());
    let sessions_tmp = tempfile::tempdir().unwrap();
    register_project(&sessions_tmp.path().join("projects"), repo_dir.path());
    let (config_dir, config) = a_config();
    let service = a_service(config, sessions_tmp.path().to_path_buf());
    World {
        repo: repo_dir.path().to_path_buf(),
        _repo_dir: repo_dir,
        _config_dir: config_dir,
        sessions_tmp,
        service,
    }
}

async fn start(world: &World, req: StartSessionRequest) -> StartSessionResponse {
    world
        .service
        .start_session(Request::new(req))
        .await
        .expect("StartSession must answer, not error")
        .into_inner()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn start_session_reports_branch_conflict_instead_of_suffixing_an_owned_branch() {
    // Given
    let world = a_world();
    git(&world.repo, &["branch", OWNED_BRANCH]);
    a_session_owning(
        world.sessions_tmp.path(),
        OWNER_SESSION,
        OWNED_BRANCH,
        true,
        "2026-07-30T09:00:00Z",
    );

    // When
    let resp = start(&world, a_new_branch_request(OWNED_BRANCH, "reject")).await;

    // Then
    assert_eq!(
        resp.session_id, "",
        "a refused creation must not report a session id"
    );
    let conflict = resp
        .branch_conflict
        .expect("an owned branch must be reported as a conflict, not suffixed");
    assert_eq!(conflict.branch, OWNED_BRANCH);
}

#[tokio::test]
async fn start_session_branch_conflict_creates_no_session_branch_or_worktree() {
    // Given
    let world = a_world();
    git(&world.repo, &["branch", OWNED_BRANCH]);
    a_session_owning(
        world.sessions_tmp.path(),
        OWNER_SESSION,
        OWNED_BRANCH,
        true,
        "2026-07-30T09:00:00Z",
    );

    // When
    start(&world, a_new_branch_request(OWNED_BRANCH, "reject")).await;

    // Then — the refusal is total: only the owning session exists, and no suffixed branch was made.
    assert_eq!(
        session_dir_count(world.sessions_tmp.path()),
        1,
        "a refused creation must leave no session directory behind"
    );
    assert_eq!(
        local_branches(&world.repo),
        vec!["feat/auth".to_string(), "main".to_string()],
        "a refused creation must create no branch"
    );
}

#[tokio::test]
async fn start_session_branch_conflict_identifies_the_owning_session() {
    // Given
    let world = a_world();
    git(&world.repo, &["branch", OWNED_BRANCH]);
    a_session_owning(
        world.sessions_tmp.path(),
        OWNER_SESSION,
        OWNED_BRANCH,
        true,
        "2026-07-30T09:00:00Z",
    );

    // When
    let resp = start(&world, a_new_branch_request(OWNED_BRANCH, "reject")).await;

    // Then — the operator needs to know which session to switch to, and whether it is running.
    let owner = resp
        .branch_conflict
        .expect("conflict must be reported")
        .owner
        .expect("conflict must name the owning session");
    assert!(owner.exists, "the owner leg must be populated");
    assert_eq!(owner.session_id, OWNER_SESSION);
    assert!(owner.is_active, "a session with a live pid is active");
    assert_eq!(owner.status, "active");
}

#[tokio::test]
async fn start_session_branch_conflict_prefers_the_active_owner_over_a_newer_idle_one() {
    // Given — two sessions claim the branch; only the older one is running.
    let world = a_world();
    git(&world.repo, &["branch", OWNED_BRANCH]);
    a_session_owning(
        world.sessions_tmp.path(),
        OWNER_SESSION,
        OWNED_BRANCH,
        true,
        "2026-07-30T09:00:00Z",
    );
    a_session_owning(
        world.sessions_tmp.path(),
        SECOND_OWNER_SESSION,
        OWNED_BRANCH,
        false,
        "2026-07-30T18:00:00Z",
    );

    // When
    let resp = start(&world, a_new_branch_request(OWNED_BRANCH, "reject")).await;

    // Then — same rule as QueryBranch: an active session wins regardless of recency.
    let owner = resp
        .branch_conflict
        .expect("conflict must be reported")
        .owner
        .expect("conflict must name the owning session");
    assert_eq!(owner.session_id, OWNER_SESSION);
}

#[tokio::test]
async fn start_session_branch_conflict_prefers_the_most_recently_updated_idle_owner() {
    // Given — two idle sessions claim the branch.
    let world = a_world();
    git(&world.repo, &["branch", OWNED_BRANCH]);
    a_session_owning(
        world.sessions_tmp.path(),
        OWNER_SESSION,
        OWNED_BRANCH,
        false,
        "2026-07-30T09:00:00Z",
    );
    a_session_owning(
        world.sessions_tmp.path(),
        SECOND_OWNER_SESSION,
        OWNED_BRANCH,
        false,
        "2026-07-30T18:00:00Z",
    );

    // When
    let resp = start(&world, a_new_branch_request(OWNED_BRANCH, "reject")).await;

    // Then
    let owner = resp
        .branch_conflict
        .expect("conflict must be reported")
        .owner
        .expect("conflict must name the owning session");
    assert_eq!(owner.session_id, SECOND_OWNER_SESSION);
}

#[tokio::test]
async fn start_session_branch_conflict_suggests_the_first_free_suffixed_branch_name() {
    // Given — the suffix the legacy path would have reached first is already taken.
    let world = a_world();
    git(&world.repo, &["branch", OWNED_BRANCH]);
    git(&world.repo, &["branch", "feat/auth-1"]);
    a_session_owning(
        world.sessions_tmp.path(),
        OWNER_SESSION,
        OWNED_BRANCH,
        true,
        "2026-07-30T09:00:00Z",
    );

    // When
    let resp = start(&world, a_new_branch_request(OWNED_BRANCH, "reject")).await;

    // Then — the suggestion pre-fills the operator's rename field, so it must be usable as-is.
    let conflict = resp.branch_conflict.expect("conflict must be reported");
    assert_eq!(conflict.suggested_branch_name, "feat/auth-2");
}

#[tokio::test]
async fn start_session_suffixes_when_the_branch_exists_but_no_session_owns_it() {
    // Given — a bare git branch with no session behind it.
    let world = a_world();
    git(&world.repo, &["branch", "feat/solo"]);

    // When
    let resp = start(&world, a_new_branch_request("feat/solo", "reject")).await;

    // Then — nothing to switch to and no agent to join, so the branch is suffixed and the session
    // starts. This is the scope boundary of the whole feature.
    assert!(
        resp.branch_conflict.is_none(),
        "a branch no session owns must not be reported as a conflict"
    );
    assert!(!resp.session_id.is_empty(), "the session must start");
    let session_dir = world
        .sessions_tmp
        .path()
        .join(SESSIONS_SUBDIR)
        .join(&resp.session_id);
    let branch = read_changeset(&session_dir)
        .expect("changeset must be written")
        .branch
        .expect("the session must be on a branch");
    assert_eq!(branch, "feat/solo-1");
}

#[tokio::test]
async fn start_session_suffixes_an_owned_branch_when_on_branch_conflict_is_empty() {
    // Given — the same owned branch as the headline test, but the caller did not opt in.
    let world = a_world();
    git(&world.repo, &["branch", OWNED_BRANCH]);
    a_session_owning(
        world.sessions_tmp.path(),
        OWNER_SESSION,
        OWNED_BRANCH,
        true,
        "2026-07-30T09:00:00Z",
    );

    // When
    let resp = start(&world, a_new_branch_request(OWNED_BRANCH, "")).await;

    // Then — existing callers keep today's behaviour; the guard is opt-in.
    assert!(
        resp.branch_conflict.is_none(),
        "a caller that did not ask to be rejected must not be"
    );
    assert!(!resp.session_id.is_empty(), "the session must start");
    let session_dir = world
        .sessions_tmp
        .path()
        .join(SESSIONS_SUBDIR)
        .join(&resp.session_id);
    let branch = read_changeset(&session_dir)
        .expect("changeset must be written")
        .branch
        .expect("the session must be on a branch");
    assert_eq!(branch, "feat/auth-1");
}

#[tokio::test]
async fn start_session_work_on_selected_branch_shares_the_owning_sessions_worktree() {
    // Given — the owning session's worktree for the branch already exists on disk. This is what the
    // operator's "add another agent" choice re-submits, so it must land on the same checkout.
    let world = a_world();
    let owner_worktree = world.repo.parent().unwrap().join("feat-auth-owner");
    git(
        &world.repo,
        &[
            "worktree",
            "add",
            "-b",
            OWNED_BRANCH,
            owner_worktree.to_str().unwrap(),
            "main",
        ],
    );
    a_session_owning(
        world.sessions_tmp.path(),
        OWNER_SESSION,
        OWNED_BRANCH,
        true,
        "2026-07-30T09:00:00Z",
    );

    // When
    let resp = start(
        &world,
        StartSessionRequest {
            session_token: VALID_TOKEN.to_string(),
            project_id: TEST_PROJECT_ID.to_string(),
            session_type: "claude-cli".to_string(),
            model: TEST_MODEL.to_string(),
            branch_worktree_intent: "work_on_selected_branch".to_string(),
            selected_branch_to_work_on: OWNED_BRANCH.to_string(),
            on_branch_conflict: "reject".to_string(),
            ..Default::default()
        },
    )
    .await;

    // Then — joining an existing branch is never refused, and reuses its worktree.
    assert!(
        resp.branch_conflict.is_none(),
        "work_on_selected_branch is the intent that deliberately joins an owned branch"
    );
    let session_dir = world
        .sessions_tmp
        .path()
        .join(SESSIONS_SUBDIR)
        .join(&resp.session_id);
    let cs = read_changeset(&session_dir).expect("changeset must be written");
    assert_eq!(
        cs.worktree.as_deref(),
        Some(
            as_the_daemon_records_it(&owner_worktree)
                .to_string_lossy()
                .as_ref()
        ),
        "the second agent must share the owning session's worktree"
    );
}
