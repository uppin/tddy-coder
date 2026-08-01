//! Acceptance: a pr-stack orchestrator's repo root comes from its session metadata.
//!
//! An orchestrator is a planning session — it never creates a worktree, so nothing ever writes
//! `repo_path` into its `changeset.yaml`. Only the `.session.yaml` written at session start names the
//! checkout it plans over. Reading the changeset alone leaves the daemon pointing at the session
//! directory itself, which is not a git repository, and every repo-derived leg of `QueryBranch`
//! then degrades in the most misleading way available to it: `git remote get-url origin` fails, so
//! `owner/repo` never resolves and the PR leg answers `exists = false` — *this branch has no PR* —
//! for a branch whose PR is open or merged. That is what hid a merged PR from its planned-PR row
//! while the row's own branch, worktree and remote were all present on disk.
//!
//! The rule these tests pin: an unresolvable repo root is *unknown* (`unavailable`, with a reason),
//! and a repo root recorded anywhere in the session directory is resolvable.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md (C2, C3, D4, D8).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use tddy_core::output::SESSIONS_SUBDIR;
use tddy_daemon::cli_session_manager::CliSessionManager;
use tddy_daemon::connection_service::{
    ConnectionServiceImpl, SessionUserResolver, SessionsBaseResolver,
};
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    BranchResolution, ConnectionService as ConnectionServiceTrait, GetPrStatusRequest,
    PrStatusView, QueryBranchRequest,
};
use tddy_testing_commons::{a_changeset, a_session_metadata, fs::write_session_yaml};

const ORCHESTRATOR: &str = "orchestrator-1";
const TOKEN: &str = "valid-session-token";
const BRANCH: &str = "feature/attach-docs/attach-proto";

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A repo on `master`, with itself as `origin`, and a GitHub-shaped remote URL so the PR path is
/// exercised (it never reaches the network in these tests — no credential is ever resolvable).
fn a_repo_with_a_github_origin(repo: &Path) {
    std::fs::create_dir_all(repo).unwrap();
    git(repo, &["init", "--initial-branch=master"]);
    git(repo, &["config", "user.email", "t@t.com"]);
    git(repo, &["config", "user.name", "T"]);
    std::fs::write(repo.join("f"), "x").unwrap();
    git(repo, &["add", "f"]);
    git(repo, &["commit", "-m", "initial"]);
    git(repo, &["remote", "add", "origin", repo.to_str().unwrap()]);
    git(repo, &["push", "origin", "master"]);
    // Push stays local (the repo is its own remote), while `git remote get-url origin` — the read the
    // daemon derives `owner/repo` from — reports a GitHub namespace, so the PR path is exercised.
    git(
        repo,
        &[
            "remote",
            "set-url",
            "--push",
            "origin",
            repo.to_str().unwrap(),
        ],
    );
    git(
        repo,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/acme/demo.git",
        ],
    );
}

/// Commit `BRANCH` in `repo` and push it to `origin`, as a child session's first push would.
fn a_branch_pushed_to_origin(repo: &Path) -> String {
    git(repo, &["checkout", "-b", BRANCH]);
    std::fs::write(repo.join("g"), BRANCH).unwrap();
    git(repo, &["add", "g"]);
    git(repo, &["commit", "-m", "work"]);
    let sha = git(repo, &["rev-parse", BRANCH]);
    git(repo, &["checkout", "master"]);
    git(repo, &["push", "origin", BRANCH]);
    sha
}

/// The linked worktree a child session works `BRANCH` in, at `.worktrees/<slug>` as the daemon
/// creates it. Returns its canonical path.
fn a_worktree_for_the_branch(repo: &Path) -> PathBuf {
    let path = repo.join(".worktrees").join(BRANCH.replace('/', "-"));
    git(repo, &["worktree", "add", path.to_str().unwrap(), BRANCH]);
    path.canonicalize().expect("worktree path must exist")
}

/// A daemon config mapping GitHub login `u` to OS user `u`, with real (non-stub) GitHub auth.
fn a_config() -> (tddy_daemon::config::DaemonConfig, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(
        &path,
        "users:\n  - github_user: \"u\"\n    os_user: \"u\"\n",
    )
    .unwrap();
    (tddy_daemon::config::DaemonConfig::load(&path).unwrap(), dir)
}

/// A service rooted at `sessions_base`, holding **no** GitHub token store — so a PR lookup that is
/// actually attempted stops at the missing credential and never reaches the network.
fn a_service(sessions_base: PathBuf) -> ConnectionServiceImpl {
    let (config, _config_dir) = a_config();
    // The config file's own temp dir may drop here; `DaemonConfig` is fully parsed by now.
    let base = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
    let user_resolver: SessionUserResolver =
        Arc::new(|token| (token == TOKEN).then(|| "u".to_string()));
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

/// A pr-stack orchestrator exactly as the daemon writes one: a recipe in `changeset.yaml` with no
/// `repo_path`, and the checkout it plans over recorded only in `.session.yaml`.
fn a_pr_stack_orchestrator_recording_its_repo_only_in_session_metadata(
    sessions_base: &Path,
    repo_root: &Path,
) {
    let dir = an_orchestrator_dir(sessions_base);
    write_session_yaml(
        &dir,
        &a_session_metadata()
            .with_session_id(ORCHESTRATOR)
            .with_repo_path(repo_root.to_string_lossy().into_owned())
            .with_pid(0)
            .build(),
    );
}

/// A pr-stack orchestrator whose session directory names no checkout at all — a legacy session,
/// written before `.session.yaml` carried `repo_path`.
fn a_pr_stack_orchestrator_recording_no_repo_at_all(sessions_base: &Path) {
    let dir = an_orchestrator_dir(sessions_base);
    write_session_yaml(
        &dir,
        &a_session_metadata()
            .with_session_id(ORCHESTRATOR)
            .with_pid(0)
            .build(),
    );
}

/// The orchestrator's session directory, with the `pr-stack` changeset every variant shares.
fn an_orchestrator_dir(sessions_base: &Path) -> PathBuf {
    let dir = sessions_base.join(SESSIONS_SUBDIR).join(ORCHESTRATOR);
    std::fs::create_dir_all(&dir).unwrap();
    tddy_core::write_changeset(&dir, &a_changeset().with_recipe("pr-stack").build()).unwrap();
    dir
}

async fn query(service: &ConnectionServiceImpl) -> BranchResolution {
    service
        .query_branch(Request::new(QueryBranchRequest {
            session_token: TOKEN.to_string(),
            session_id: ORCHESTRATOR.to_string(),
            branch: BRANCH.to_string(),
            // This file's subject is how the repo root is resolved, not the base comparison.
            base_branch: String::new(),
        }))
        .await
        .expect("QueryBranch must succeed as an RPC")
        .into_inner()
        .resolution
        .expect("a resolution must be returned")
}

async fn pr_status(service: &ConnectionServiceImpl) -> PrStatusView {
    service
        .get_pr_status(Request::new(GetPrStatusRequest {
            session_token: TOKEN.to_string(),
            session_id: ORCHESTRATOR.to_string(),
            branch: BRANCH.to_string(),
        }))
        .await
        .expect("GetPrStatus must succeed as an RPC")
        .into_inner()
        .status
        .expect("a status must be returned")
}

#[tokio::test]
async fn finds_the_remote_ref_of_a_pushed_branch_for_an_orchestrator_whose_repo_is_only_in_session_metadata(
) {
    // Given — the stack's repo is named only in `.session.yaml`, and the branch is on origin
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    a_repo_with_a_github_origin(&repo);
    let sha = a_branch_pushed_to_origin(&repo);
    let sessions_base = temp.path().join("data");
    a_pr_stack_orchestrator_recording_its_repo_only_in_session_metadata(&sessions_base, &repo);

    // When
    let resolution = query(&a_service(sessions_base)).await;

    // Then — descendants of this node can be based onto it
    let remote = resolution.remote.expect("the remote leg must be present");
    assert_eq!((remote.exists, remote.sha), (true, sha));
}

#[tokio::test]
async fn finds_the_worktree_of_a_branch_for_an_orchestrator_whose_repo_is_only_in_session_metadata()
{
    // Given — a worktree checked out for the branch in the stack's repo
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    a_repo_with_a_github_origin(&repo);
    a_branch_pushed_to_origin(&repo);
    let worktree_path = a_worktree_for_the_branch(&repo);
    let sessions_base = temp.path().join("data");
    a_pr_stack_orchestrator_recording_its_repo_only_in_session_metadata(&sessions_base, &repo);

    // When
    let resolution = query(&a_service(sessions_base)).await;

    // Then — the row shows where the work lives
    let worktree = resolution
        .worktree
        .expect("the worktree leg must be present");
    assert_eq!(
        (worktree.exists, PathBuf::from(worktree.path)),
        (true, worktree_path)
    );
}

#[tokio::test]
async fn reports_unavailable_rather_than_no_pr_for_an_orchestrator_whose_repo_is_only_in_session_metadata(
) {
    // Given — the stack's GitHub namespace is resolvable through `.session.yaml`, but this real
    // login's token was never retained, so the lookup itself cannot be performed
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    a_repo_with_a_github_origin(&repo);
    a_branch_pushed_to_origin(&repo);
    let sessions_base = temp.path().join("data");
    a_pr_stack_orchestrator_recording_its_repo_only_in_session_metadata(&sessions_base, &repo);

    // When
    let resolution = query(&a_service(sessions_base)).await;

    // Then — the credential is the only thing missing; the row must not claim the branch has no PR
    let pr = resolution.pr.expect("the pr leg must be present");
    assert_eq!((pr.exists, pr.unavailable), (false, true));
    assert!(
        !pr.unavailable_reason.trim().is_empty(),
        "unavailability must carry an operator-facing reason"
    );
}

#[tokio::test]
async fn reports_unavailable_rather_than_no_pr_when_no_file_records_the_orchestrators_repo() {
    // Given — an orchestrator whose session directory names no checkout at all
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().join("data");
    a_pr_stack_orchestrator_recording_no_repo_at_all(&sessions_base);

    // When
    let resolution = query(&a_service(sessions_base)).await;

    // Then — an unknown repo makes the PR status unknown, not absent
    let pr = resolution.pr.expect("the pr leg must be present");
    assert_eq!((pr.exists, pr.unavailable), (false, true));
    assert!(
        !pr.unavailable_reason.trim().is_empty(),
        "unavailability must carry an operator-facing reason"
    );
}

#[tokio::test]
async fn get_pr_status_reports_unavailable_rather_than_no_pr_for_an_orchestrator_whose_repo_is_only_in_session_metadata(
) {
    // Given — the same orchestrator, queried through the single-status RPC
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    a_repo_with_a_github_origin(&repo);
    a_branch_pushed_to_origin(&repo);
    let sessions_base = temp.path().join("data");
    a_pr_stack_orchestrator_recording_its_repo_only_in_session_metadata(&sessions_base, &repo);

    // When
    let status = pr_status(&a_service(sessions_base)).await;

    // Then — both PR-status entry points resolve the repo the same way
    assert_eq!((status.exists, status.unavailable), (false, true));
    assert!(
        !status.unavailable_reason.trim().is_empty(),
        "unavailability must carry an operator-facing reason"
    );
}
