//! Acceptance: the daemon's half of seeding a PR stack from an existing session.
//!
//! Two responsibilities, and they are deliberately separate.
//!
//! **Forwarding.** The base session id reaches the spawned `tddy-coder` as
//! `--stack-seed-base-session`, on exactly the path `--stack-parent` already takes. Both flags are
//! built by one pure function rather than appended inline to a `Command`, because a flag that is
//! silently not passed is the failure mode here — the orchestrator comes up looking successful and
//! carrying an empty stack — and an inline `cmd.arg` sequence gives a test nothing to assert on.
//!
//! **Refusing early.** A base session that cannot be resolved, or one asked for alongside a recipe
//! that is not `pr-stack`, is refused *before* anything spawns. This is what lets the new-session
//! form show the reason in its error strip: a refusal raised after the spawn is invisible to the
//! form, which has already navigated away. It is also why the seeding function refuses again on its
//! own — the CLI flag is reachable without this RPC.
//!
//! Both seams are pure functions, and B11 is why that is not the whole story: it drives a real
//! `StartSession` so that `StartSessionRequest.pr_stack_base_session_id` is shown to *reach* the
//! refusal. Every test that calls the seams directly would still pass with the request field dropped
//! on the floor.
//!
//! Feature: `docs/ft/coder/pr-stacking.md#seeding-the-stack-from-an-existing-session-added-2026-08-13`

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_core::changeset::{write_changeset, Changeset};
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::cli_session_manager::CliSessionManager;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::{
    validate_stack_seed_base_session, ConnectionServiceImpl, SessionUserResolver,
    SessionsBaseResolver,
};
use tddy_daemon::spawner::pr_stack_spawn_args;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, StartSessionRequest,
};

const BASE_SESSION: &str = "session-auth-store";
const BASE_BRANCH: &str = "feat/auth-store";
const VALID_TOKEN: &str = "valid-token";
const TEST_PROJECT_ID: &str = "test-project";

// --- fixtures ---------------------------------------------------------------

fn a_session(sessions_base: &Path, session_id: &str, changeset: Changeset) {
    let dir = unified_session_dir_path(sessions_base, session_id);
    std::fs::create_dir_all(&dir).unwrap();
    write_changeset(&dir, &changeset).unwrap();
}

/// A code session working on `branch` inside `repo` — the kind that can seed a stack for a project
/// whose repository is `repo`.
fn a_session_on_branch_in(sessions_base: &Path, session_id: &str, branch: &str, repo: &Path) {
    a_session(
        sessions_base,
        session_id,
        Changeset {
            branch: Some(branch.to_string()),
            repo_path: Some(repo.display().to_string()),
            ..Changeset::default()
        },
    );
}

/// A directory standing in for a project's repository.
///
/// Real, not a made-up path: the repository check canonicalizes both sides, because a project
/// registered through a symlink and a session that recorded the resolved path name the same repository
/// and a string comparison would call them different.
fn a_repo_dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

// --- validation assertions --------------------------------------------------

/// A refused call, so each assertion is one fluent line instead of a `Status` unwrapped by hand.
///
/// Normalized over the two status types this feature's refusals travel as — the pre-spawn validator
/// answers a `tonic::Status` (it feeds the tonic adapter), while `StartSession` answers a
/// `tddy_rpc::Status` — so one shape covers the seam and the RPC that calls it. The code is compared by
/// name, and named at the call site with the enum the surface under test actually returns.
struct Refusal {
    code: String,
    reason: String,
}

/// The two `Status` flavours a refusal can arrive as.
trait RefusedStatus {
    fn into_refusal(self) -> Refusal;
}

impl RefusedStatus for tonic::Status {
    fn into_refusal(self) -> Refusal {
        Refusal {
            code: format!("{:?}", self.code()),
            reason: self.message().to_string(),
        }
    }
}

impl RefusedStatus for tddy_rpc::Status {
    fn into_refusal(self) -> Refusal {
        Refusal {
            code: format!("{:?}", self.code),
            reason: self.message,
        }
    }
}

fn assert_refused<T, S: RefusedStatus>(result: Result<T, S>) -> Refusal {
    match result {
        Err(status) => status.into_refusal(),
        Ok(_) => panic!("expected the base session to be refused, but it was accepted"),
    }
}

fn assert_accepted(result: Result<(), tonic::Status>) {
    if let Err(status) = result {
        let refusal = status.into_refusal();
        panic!(
            "expected the base session to be accepted, but it was refused with {}: {}",
            refusal.code, refusal.reason
        );
    }
}

impl Refusal {
    fn with_reason_containing(self, fragment: &str) -> Self {
        assert!(
            self.reason.contains(fragment),
            "expected the refusal to mention '{fragment}', was '{}'",
            self.reason
        );
        self
    }

    /// The status code, which is what the web maps to its error strip — not the prose.
    fn with_code(self, expected: impl std::fmt::Debug) -> Self {
        assert_eq!(
            self.code,
            format!("{expected:?}"),
            "refusal code mismatch (reason was '{}')",
            self.reason
        );
        self
    }
}

// --- B1, B2: forwarding to the spawned coder --------------------------------

#[test]
fn passes_the_stack_base_session_to_the_spawned_coder() {
    // Given / When a spawn that seeds its stack on an existing session
    let args = pr_stack_spawn_args(None, None, Some(BASE_SESSION));

    // Then
    assert_eq!(
        args,
        vec![
            "--stack-seed-base-session".to_string(),
            BASE_SESSION.to_string()
        ]
    );
}

#[test]
fn omits_the_flag_when_no_stack_base_session_was_requested() {
    // Given / When an ordinary spawn
    let args = pr_stack_spawn_args(None, None, None);

    // Then — the request an unseeded orchestrator has always sent
    assert_eq!(args, Vec::<String>::new());
}

#[test]
fn omits_the_flag_for_a_blank_stack_base_session() {
    // Given / When — proto3 carries "unset" as the empty string, so blank must mean unset here too
    let args = pr_stack_spawn_args(None, None, Some("   "));

    // Then
    assert_eq!(args, Vec::<String>::new());
}

#[test]
fn omits_the_flag_for_a_blank_stack_parent() {
    // Given / When — `stack_parent` reaches this from the same proto3 string field, so blank must
    // mean unset for it too
    let args = pr_stack_spawn_args(Some("  "), None, None);

    // Then
    assert_eq!(args, Vec::<String>::new());
}

#[test]
fn trims_a_padded_stack_parent() {
    // Given / When — a padded id names a real session, so it is trimmed rather than refused; the
    // coder receives an id it can resolve to a directory
    let args = pr_stack_spawn_args(Some(" orchestrator-1 "), None, None);

    // Then
    assert_eq!(
        args,
        vec!["--stack-parent".to_string(), "orchestrator-1".to_string()]
    );
}

#[test]
fn trims_a_padded_stack_base_session() {
    // Given / When
    let args = pr_stack_spawn_args(None, None, Some(" session-auth-store "));

    // Then
    assert_eq!(
        args,
        vec![
            "--stack-seed-base-session".to_string(),
            BASE_SESSION.to_string()
        ]
    );
}

#[test]
fn passes_a_stack_parent_and_a_stack_base_session_together() {
    // Given / When both are set — a child spawn is keyed on its parent, a seed on its base, and
    // neither flag may displace the other
    let args = pr_stack_spawn_args(Some("orchestrator-1"), None, Some(BASE_SESSION));

    // Then
    assert_eq!(
        args,
        vec![
            "--stack-parent".to_string(),
            "orchestrator-1".to_string(),
            "--stack-seed-base-session".to_string(),
            BASE_SESSION.to_string(),
        ]
    );
}

#[test]
fn passes_the_planned_node_id_to_the_spawned_coder() {
    // Given / When — a child spawn for a planned node, which the coder publishes as its stack
    // association so the PR-Stack view can join a cross-host child back to the row that started it
    let args = pr_stack_spawn_args(Some("orchestrator-1"), Some("n2"), None);

    // Then — the node id is the surface's own, never re-derived from the branch (D34)
    assert_eq!(
        args,
        vec![
            "--stack-parent".to_string(),
            "orchestrator-1".to_string(),
            "--stack-node-id".to_string(),
            "n2".to_string(),
        ]
    );
}

#[test]
fn omits_the_node_flag_for_a_spawn_that_names_no_planned_node() {
    // Given / When — the orchestrator agent's own `spawn-child`, which always runs on the
    // orchestrator's host and is linked by the branch-derived local write
    let args = pr_stack_spawn_args(Some("orchestrator-1"), None, None);

    // Then
    assert_eq!(
        args,
        vec!["--stack-parent".to_string(), "orchestrator-1".to_string()]
    );
}

#[test]
fn omits_the_node_flag_for_a_blank_planned_node_id() {
    // Given / When — proto3 carries "unset" as the empty string, so blank must mean unset here too
    let args = pr_stack_spawn_args(Some("orchestrator-1"), Some("   "), None);

    // Then — a blank `--stack-node-id` would reach the coder as an association it cannot publish
    assert_eq!(
        args,
        vec!["--stack-parent".to_string(), "orchestrator-1".to_string()]
    );
}

#[test]
fn trims_a_padded_planned_node_id() {
    // Given / When
    let args = pr_stack_spawn_args(Some("orchestrator-1"), Some(" n2 "), None);

    // Then
    assert_eq!(
        args,
        vec![
            "--stack-parent".to_string(),
            "orchestrator-1".to_string(),
            "--stack-node-id".to_string(),
            "n2".to_string(),
        ]
    );
}

// --- B3–B5: refusals raised before anything spawns --------------------------

#[test]
fn accepts_a_base_session_that_owns_a_branch_on_a_pr_stack_session() {
    // Given a session working on a branch in the requesting project's repository
    let sessions = tempfile::tempdir().unwrap();
    let repo = a_repo_dir();
    a_session_on_branch_in(sessions.path(), BASE_SESSION, BASE_BRANCH, repo.path());

    // When a pr-stack session asks to base its stack on it
    let result =
        validate_stack_seed_base_session(sessions.path(), "pr-stack", BASE_SESSION, repo.path());

    // Then it is accepted
    assert_accepted(result);
}

#[test]
fn accepts_a_pr_stack_session_that_names_no_base_session() {
    // Given / When — the unseeded orchestrator every existing caller creates
    let result = validate_stack_seed_base_session(
        Path::new("/nonexistent"),
        "pr-stack",
        "",
        Path::new("/nonexistent"),
    );

    // Then nothing is validated, because nothing was asked for
    assert_accepted(result);
}

#[test]
fn refuses_a_stack_base_session_when_the_recipe_is_not_pr_stack() {
    // Given a resolvable base session
    let sessions = tempfile::tempdir().unwrap();
    let repo = a_repo_dir();
    a_session_on_branch_in(sessions.path(), BASE_SESSION, BASE_BRANCH, repo.path());

    // When a `tdd` session asks to seed a stack, which it has no stack to seed
    let result =
        validate_stack_seed_base_session(sessions.path(), "tdd", BASE_SESSION, repo.path());

    // Then it is refused rather than ignored: dropping the field would create a session that looks
    // seeded and is not
    assert_refused(result)
        .with_code(tonic::Code::InvalidArgument)
        .with_reason_containing("pr-stack");
}

#[test]
fn refuses_a_stack_base_session_that_owns_no_branch() {
    // Given a session that has not created its branch yet
    let sessions = tempfile::tempdir().unwrap();
    let repo = a_repo_dir();
    a_session(sessions.path(), "session-unstarted", Changeset::default());

    // When a pr-stack session asks to base its stack on it
    let result = validate_stack_seed_base_session(
        sessions.path(),
        "pr-stack",
        "session-unstarted",
        repo.path(),
    );

    // Then it is refused before the spawn, so the form can show the reason
    assert_refused(result)
        .with_code(tonic::Code::FailedPrecondition)
        .with_reason_containing("owns no branch");
}

#[test]
fn refuses_a_stack_base_session_that_does_not_resolve() {
    // Given a sessions root with no such session
    let sessions = tempfile::tempdir().unwrap();
    let repo = a_repo_dir();

    // When a pr-stack session asks to base its stack on it
    let result =
        validate_stack_seed_base_session(sessions.path(), "pr-stack", "session-ghost", repo.path());

    // Then it is refused, naming what could not be resolved
    assert_refused(result).with_reason_containing("session-ghost");
}

#[test]
fn accepts_a_stack_base_session_on_a_legacy_pr_stack_alias() {
    // Given a resolvable base session and a session started under a legacy alias, which resolves to
    // the same recipe
    let sessions = tempfile::tempdir().unwrap();
    let repo = a_repo_dir();
    a_session_on_branch_in(sessions.path(), BASE_SESSION, BASE_BRANCH, repo.path());

    // When it asks to seed its stack
    let result = validate_stack_seed_base_session(
        sessions.path(),
        "orchestrate-pr-stack",
        BASE_SESSION,
        repo.path(),
    );

    // Then it is accepted — the alias is the same orchestrator, and refusing it would make the
    // recipe name a load-bearing string rather than a resolution
    assert_accepted(result);
}

// --- B15–B18: the base session must belong to this stack's repository -------
//
// A branch this project's repository does not have cannot be stacked on: nothing local resolves
// `origin/<branch>`, so the failure would land much later, as a git error, on the first descendant
// spawn — by which time the orchestrator exists and looks seeded. The picker narrows the choice, but
// the CLI reaches `StartSession` without it, so the refusal has to live here.

#[test]
fn refuses_a_base_session_that_works_in_another_repository() {
    // Given a session on a branch in a repository that is not the requesting project's
    let sessions = tempfile::tempdir().unwrap();
    let project_repo = a_repo_dir();
    let other_repo = a_repo_dir();
    a_session_on_branch_in(
        sessions.path(),
        BASE_SESSION,
        BASE_BRANCH,
        other_repo.path(),
    );

    // When a pr-stack session in the project asks to base its stack on it
    let result = validate_stack_seed_base_session(
        sessions.path(),
        "pr-stack",
        BASE_SESSION,
        project_repo.path(),
    );

    // Then it is refused up front, naming both repositories — the alternative is a git error on the
    // first descendant of an orchestrator that already exists
    assert_refused(result)
        .with_code(tonic::Code::FailedPrecondition)
        .with_reason_containing("not this project's")
        .with_reason_containing(&other_repo.path().display().to_string());
}

/// A claude-cli / cursor-cli / workspace session records its **own worktree** as `repo_path`
/// (`<repo>/.worktrees/<name>`), not the repository root a `tddy-coder` session records. Both work in
/// the project's repository, so the relation the check enforces is "at or under" — an equality
/// comparison would refuse every session type but one.
#[test]
fn accepts_a_base_session_working_in_a_worktree_of_the_projects_repository() {
    // Given a session whose recorded repository is a worktree inside the project's repository
    let sessions = tempfile::tempdir().unwrap();
    let project_repo = a_repo_dir();
    let worktree = project_repo
        .path()
        .join(".worktrees")
        .join("feat-auth-store");
    std::fs::create_dir_all(&worktree).unwrap();
    a_session_on_branch_in(sessions.path(), BASE_SESSION, BASE_BRANCH, &worktree);

    // When a pr-stack session in that project asks to base its stack on it
    let result = validate_stack_seed_base_session(
        sessions.path(),
        "pr-stack",
        BASE_SESSION,
        project_repo.path(),
    );

    // Then it is accepted — the branch lives in this project's repository, whichever checkout of it
    // the session works in
    assert_accepted(result);
}

#[test]
fn refuses_a_base_session_that_records_no_repository() {
    // Given a session on a branch whose changeset names no repository at all
    let sessions = tempfile::tempdir().unwrap();
    let project_repo = a_repo_dir();
    a_session(
        sessions.path(),
        BASE_SESSION,
        Changeset {
            branch: Some(BASE_BRANCH.to_string()),
            ..Changeset::default()
        },
    );

    // When a pr-stack session asks to base its stack on it
    let result = validate_stack_seed_base_session(
        sessions.path(),
        "pr-stack",
        BASE_SESSION,
        project_repo.path(),
    );

    // Then it is refused rather than assumed to be in this project: "could not tell" is not
    // "same repository", and accepting it would defer the failure to the first descendant's spawn
    assert_refused(result)
        .with_code(tonic::Code::FailedPrecondition)
        .with_reason_containing("records no repository");
}

#[test]
fn refuses_a_base_session_already_owned_by_another_orchestrators_stack() {
    // Given a session on a branch that another orchestrator already tracks as one of its nodes
    let sessions = tempfile::tempdir().unwrap();
    let repo = a_repo_dir();
    a_session(
        sessions.path(),
        BASE_SESSION,
        Changeset {
            branch: Some(BASE_BRANCH.to_string()),
            repo_path: Some(repo.path().display().to_string()),
            orchestrator_session_id: Some("orchestrator-elsewhere".to_string()),
            ..Changeset::default()
        },
    );

    // When a second pr-stack session asks to base its stack on it
    let result =
        validate_stack_seed_base_session(sessions.path(), "pr-stack", BASE_SESSION, repo.path());

    // Then it is refused: two orchestrators with repoint and pull authority over one branch is
    // ambiguous ownership, and refusing is the recoverable direction
    assert_refused(result)
        .with_code(tonic::Code::FailedPrecondition)
        .with_reason_containing("already a node")
        .with_reason_containing("orchestrator-elsewhere");
}

// --- B11: the request field reaches the refusal ------------------------------

/// The OS user the test process runs as — the config must map a real user, because a request that got
/// past the refusal would spawn as that user.
fn current_os_user() -> String {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    assert!(!pw.is_null(), "current uid must resolve to a passwd entry");
    unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }
        .to_string_lossy()
        .into_owned()
}

/// A daemon config that maps the test user and configures LiveKit.
///
/// LiveKit is configured even though a refused start never reaches it: a tool session with no LiveKit
/// creds is refused with `FailedPrecondition("LiveKit not configured")`, which would let B11 pass for
/// the wrong reason if `StartSession` ever stopped reading `pr_stack_base_session_id`. No server has to
/// be running — resolving the creds from config is all that happens before the refusal under test.
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
livekit:
  url: ws://127.0.0.1:7880
  api_key: devkey
  api_secret: devsecret
"#
    );
    let config_path = dir.path().join("daemon.yaml");
    std::fs::write(&config_path, yaml).unwrap();
    (
        dir,
        DaemonConfig::load(&config_path).expect("config must parse"),
    )
}

fn a_service(config: DaemonConfig, sessions_base: PathBuf) -> ConnectionServiceImpl {
    let base = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
    let resolved_user = current_os_user();
    let user_resolver: SessionUserResolver =
        Arc::new(move |token| (token == VALID_TOKEN).then(|| resolved_user.clone()));
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        sessions_base,
        user_resolver,
        None,
        None,
        None,
        Arc::new(CliSessionManager::new()),
    )
}

/// A git repo the registered project points at, so project resolution is never what fails.
fn a_repo(dir: &Path) {
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .output()
            .expect("git must run");
    };
    git(&["init", "-b", "main"]);
    git(&["commit", "--allow-empty", "-m", "init"]);
    git(&["remote", "add", "origin", dir.to_str().unwrap()]);
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

/// The session directories that exist — a refusal must add none.
fn session_ids(sessions_base: &Path) -> Vec<String> {
    let mut ids: Vec<String> = match std::fs::read_dir(sessions_base.join(SESSIONS_SUBDIR)) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => vec![],
    };
    ids.sort();
    ids
}

/// The request the new-session form sends for a `pr-stack` orchestrator seeded on an existing session.
fn a_pr_stack_start_request(base_session_id: &str) -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: "tool".to_string(),
        recipe: "pr-stack".to_string(),
        pr_stack_base_session_id: base_session_id.to_string(),
        ..Default::default()
    }
}

/// **B11** — the one test that pins the *wiring*: a `StartSessionRequest` naming a branchless base
/// session is answered with the pre-spawn refusal, and creates nothing.
///
/// Every other refusal test here calls `validate_stack_seed_base_session` directly, so all of them
/// would still pass if `StartSession` never read `pr_stack_base_session_id` — which is precisely this
/// feature's failure mode, an orchestrator that comes up looking successful and carrying an empty
/// stack.
#[tokio::test]
async fn start_session_refuses_a_branchless_base_session_and_creates_no_session() {
    // Given a registered project, and a session that has not created its branch yet
    let repo_dir = tempfile::tempdir().unwrap();
    a_repo(repo_dir.path());
    let sessions = tempfile::tempdir().unwrap();
    register_project(&sessions.path().join("projects"), repo_dir.path());
    a_session(sessions.path(), "session-unstarted", Changeset::default());
    let (_config_dir, config) = a_config();
    let service = a_service(config, sessions.path().to_path_buf());

    // When the form asks for an orchestrator based on it
    let result = service
        .start_session(Request::new(a_pr_stack_start_request("session-unstarted")))
        .await;

    // Then the RPC carries the reason the form shows, and nothing was created
    assert_refused(result)
        .with_code(tddy_rpc::Code::FailedPrecondition)
        .with_reason_containing("owns no branch")
        .with_reason_containing("session-unstarted");
    assert_eq!(
        session_ids(sessions.path()),
        vec!["session-unstarted".to_string()],
        "a refused start must leave only the base session behind"
    );
}

/// The wiring test for the repository scoping: `StartSession` must compare the base session against
/// **the requesting project's** repository.
///
/// Calling the validator directly cannot show that. It takes the project's repository root as an
/// argument, so every seam test above passes just as well if `StartSession` resolved that root from
/// the wrong place — or handed over a path that matches everything.
#[tokio::test]
async fn start_session_refuses_a_base_session_from_another_repository_and_creates_no_session() {
    // Given a registered project, and a session on a branch in a *different* repository
    let repo_dir = tempfile::tempdir().unwrap();
    a_repo(repo_dir.path());
    let other_repo = a_repo_dir();
    let sessions = tempfile::tempdir().unwrap();
    register_project(&sessions.path().join("projects"), repo_dir.path());
    a_session_on_branch_in(
        sessions.path(),
        BASE_SESSION,
        BASE_BRANCH,
        other_repo.path(),
    );
    let (_config_dir, config) = a_config();
    let service = a_service(config, sessions.path().to_path_buf());

    // When the form asks for an orchestrator based on it
    let result = service
        .start_session(Request::new(a_pr_stack_start_request(BASE_SESSION)))
        .await;

    // Then the RPC carries the reason, and nothing was created — the alternative is an orchestrator
    // that looks seeded until its first descendant fails to resolve `origin/<branch>`
    assert_refused(result)
        .with_code(tddy_rpc::Code::FailedPrecondition)
        .with_reason_containing("not this project's")
        .with_reason_containing(BASE_SESSION);
    assert_eq!(
        session_ids(sessions.path()),
        vec![BASE_SESSION.to_string()],
        "a refused start must leave only the base session behind"
    );
}
