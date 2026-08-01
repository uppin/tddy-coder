//! Acceptance: `QueryBranch` resolves a branch honestly, and never collapses into an error.
//!
//! The PR-Stack screen polls this one call for everything it knows about a branch. Two rules:
//!
//! - the **remote** leg reports whether `origin/<branch>` exists, because that — not a non-empty
//!   branch name — is what decides whether a descendant's worktree can be based onto it (C2, D4),
//! - a PR lookup that cannot be performed degrades the **pr** leg alone (D8). It used to fail the
//!   whole RPC, and the web's `.catch()` then discarded the entire resolution — silently losing the
//!   session, worktree and remote legs too.
//!
//! Stub/demo authentication is a first-class state, not a degraded one: it reports no PRs and no
//! error at all (D12).
//!
//! The **base_sync** leg follows the same rule one level down: a comparison that could not be made
//! carries an explicit `unavailable` discriminator, because a failed comparison is byte-identical to
//! a healthy one — nothing behind, no conflicts — and rendering it as "clean" is exactly the
//! conflation D12 already ruled out for PR status (D27). An unnamed base is likewise unavailable
//! rather than substituted with the project default (D29): the number beside a row must describe the
//! same base the row's own base line names.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md (C2, C3, D4, D8, D12) and
//! docs/ft/coder/1-WIP/PRD-2026-08-01-pr-stack-panel-ux.md (C4, D27-D29).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use tddy_core::changeset::Changeset;
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_daemon::cli_session_manager::CliSessionManager;
use tddy_daemon::connection_service::{
    ConnectionServiceImpl, SessionUserResolver, SessionsBaseResolver,
};
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    BranchBaseSync, BranchResolution, ConnectionService as ConnectionServiceTrait,
    QueryBranchRequest,
};
use tddy_testing_commons::{a_session_metadata, fs::write_session_yaml};

const ORCHESTRATOR: &str = "orchestrator-1";
const CHILD: &str = "child-1";
const TOKEN: &str = "valid-session-token";
const BRANCH: &str = "feature/attach-docs/attach-store";

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

/// Commit `branch` in `repo`; push it to `origin` only when `push` is set.
fn a_branch(repo: &Path, branch: &str, push: bool) -> String {
    git(repo, &["checkout", "-b", branch]);
    std::fs::write(repo.join("g"), branch).unwrap();
    git(repo, &["add", "g"]);
    git(repo, &["commit", "-m", "work"]);
    let sha = git(repo, &["rev-parse", branch]);
    git(repo, &["checkout", "master"]);
    if push {
        git(repo, &["push", "origin", branch]);
    }
    sha
}

/// A commit landed on `master` after the branch forked, pushed — so `origin/master` runs ahead of
/// the branch by exactly one commit.
fn a_commit_on_master_touching(repo: &Path, file: &str, contents: &str) {
    git(repo, &["checkout", "master"]);
    std::fs::write(repo.join(file), contents).unwrap();
    git(repo, &["add", file]);
    git(repo, &["commit", "-m", "base work"]);
    git(repo, &["push", "origin", "master"]);
}

/// The base comparison of a resolution, which every base-sync case asserts on.
fn base_sync_of(resolution: &BranchResolution) -> BranchBaseSync {
    resolution
        .base_sync
        .clone()
        .expect("the base-sync leg must be present")
}

/// A daemon config mapping GitHub login `u` to OS user `u`, with `github.stub` as given.
fn a_config(stub_github: bool) -> (tddy_daemon::config::DaemonConfig, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let github = if stub_github {
        "github:\n  stub: true\n"
    } else {
        ""
    };
    let path = dir.path().join("config.yaml");
    std::fs::write(
        &path,
        format!("users:\n  - github_user: \"u\"\n    os_user: \"u\"\n{github}"),
    )
    .unwrap();
    (tddy_daemon::config::DaemonConfig::load(&path).unwrap(), dir)
}

/// A service rooted at `sessions_base`, holding **no** GitHub token store — the deployment state
/// this feature is about: a real login whose credential the daemon never retained.
fn a_service(sessions_base: PathBuf, stub_github: bool) -> ConnectionServiceImpl {
    let (config, _config_dir) = a_config(stub_github);
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

fn write_changeset(sessions_base: &Path, session_id: &str, changeset: &Changeset) {
    let dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
    std::fs::create_dir_all(&dir).unwrap();
    tddy_core::write_changeset(&dir, changeset).unwrap();
}

/// A pr-stack orchestrator over `repo_root`, plus a child session working `BRANCH`.
fn a_pr_stack_orchestrator_with_a_child_on(sessions_base: &Path, repo_root: &Path) {
    write_changeset(
        sessions_base,
        ORCHESTRATOR,
        &Changeset {
            recipe: Some("pr-stack".to_string()),
            repo_path: Some(repo_root.to_string_lossy().into_owned()),
            ..Changeset::default()
        },
    );
    write_changeset(
        sessions_base,
        CHILD,
        &Changeset {
            branch: Some(BRANCH.to_string()),
            ..Changeset::default()
        },
    );
    write_session_yaml(
        &sessions_base.join(SESSIONS_SUBDIR).join(CHILD),
        &a_session_metadata()
            .with_session_id(CHILD)
            .with_pid(0)
            .build(),
    );
}

/// Resolve `branch` without asking for any base comparison — what a caller that has no base to name
/// sends.
async fn query(service: &ConnectionServiceImpl, branch: &str) -> BranchResolution {
    query_against(service, branch, "").await
}

/// Resolve `branch`, comparing it against `base_branch`.
async fn query_against(
    service: &ConnectionServiceImpl,
    branch: &str,
    base_branch: &str,
) -> BranchResolution {
    service
        .query_branch(Request::new(QueryBranchRequest {
            session_token: TOKEN.to_string(),
            session_id: ORCHESTRATOR.to_string(),
            branch: branch.to_string(),
            base_branch: base_branch.to_string(),
        }))
        .await
        .expect("QueryBranch must succeed as an RPC")
        .into_inner()
        .resolution
        .expect("a resolution must be returned")
}

#[tokio::test]
async fn reports_the_remote_ref_of_a_branch_that_is_pushed_to_origin() {
    // Given — the branch exists on origin, so a descendant can be based onto it
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    a_repo_with_a_github_origin(&repo);
    let sha = a_branch(&repo, BRANCH, true);
    let sessions_base = temp.path().join("data");
    a_pr_stack_orchestrator_with_a_child_on(&sessions_base, &repo);

    // When
    let resolution = query(&a_service(sessions_base, false), BRANCH).await;

    // Then
    let remote = resolution.remote.expect("the remote leg must be present");
    assert_eq!((remote.exists, remote.sha), (true, sha));
}

#[tokio::test]
async fn reports_no_remote_ref_for_a_branch_that_was_never_pushed() {
    // Given — the branch exists only locally; `git fetch origin <branch>` would fail for a child
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    a_repo_with_a_github_origin(&repo);
    a_branch(&repo, BRANCH, false);
    let sessions_base = temp.path().join("data");
    a_pr_stack_orchestrator_with_a_child_on(&sessions_base, &repo);

    // When
    let resolution = query(&a_service(sessions_base, false), BRANCH).await;

    // Then — the row shows "Missing branch" instead of offering a spawn that would fail
    let remote = resolution.remote.expect("the remote leg must be present");
    assert_eq!(
        (remote.exists, remote.sha.as_str()),
        (false, ""),
        "an unpushed branch must not read as available on origin"
    );
}

#[tokio::test]
async fn keeps_the_session_and_remote_legs_when_the_pr_lookup_cannot_be_performed() {
    // Given — a real (non-stub) login whose GitHub token was never retained
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    a_repo_with_a_github_origin(&repo);
    a_branch(&repo, BRANCH, true);
    let sessions_base = temp.path().join("data");
    a_pr_stack_orchestrator_with_a_child_on(&sessions_base, &repo);

    // When
    let resolution = query(&a_service(sessions_base, false), BRANCH).await;

    // Then — the legs that do not depend on GitHub are still answered
    let session = resolution.session.expect("the session leg must be present");
    let remote = resolution.remote.expect("the remote leg must be present");
    assert_eq!(
        (
            resolution.branch.as_str(),
            session.exists,
            session.session_id.as_str(),
            remote.exists
        ),
        (BRANCH, true, CHILD, true)
    );
}

#[tokio::test]
async fn reports_the_pr_status_as_unavailable_when_no_github_token_is_retained() {
    // Given — a real login with no stored credential
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    a_repo_with_a_github_origin(&repo);
    a_branch(&repo, BRANCH, true);
    let sessions_base = temp.path().join("data");
    a_pr_stack_orchestrator_with_a_child_on(&sessions_base, &repo);

    // When
    let resolution = query(&a_service(sessions_base, false), BRANCH).await;

    // Then — the operator learns the status is unknown, not that no PR exists
    let pr = resolution.pr.expect("the pr leg must be present");
    assert_eq!((pr.exists, pr.unavailable), (false, true));
    assert!(
        !pr.unavailable_reason.trim().is_empty(),
        "unavailability must carry an operator-facing reason"
    );
}

#[tokio::test]
async fn reports_no_pr_and_no_unavailability_for_a_stub_login() {
    // Given — a daemon running `github.stub: true` so the product can be demoed
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    a_repo_with_a_github_origin(&repo);
    a_branch(&repo, BRANCH, true);
    let sessions_base = temp.path().join("data");
    a_pr_stack_orchestrator_with_a_child_on(&sessions_base, &repo);

    // When
    let resolution = query(&a_service(sessions_base, true), BRANCH).await;

    // Then — a demo shows no PRs, indistinguishable from a repository that has none
    let pr = resolution.pr.expect("the pr leg must be present");
    assert_eq!(
        (pr.exists, pr.unavailable, pr.unavailable_reason.as_str()),
        (false, false, ""),
        "a stub login must never surface an error or an unavailable PR status"
    );
}

// ---------------------------------------------------------------------------
// The base-sync leg
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reports_how_far_behind_its_base_a_branch_is_and_which_base_that_was() {
    // Given — the branch forked, then master landed a commit it does not have
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    a_repo_with_a_github_origin(&repo);
    a_branch(&repo, BRANCH, true);
    a_commit_on_master_touching(&repo, "base-only", "base work");
    let sessions_base = temp.path().join("data");
    a_pr_stack_orchestrator_with_a_child_on(&sessions_base, &repo);

    // When — the caller names the base the way a project stores it, remote prefix and all
    let resolution = query_against(&a_service(sessions_base, false), BRANCH, "origin/master").await;

    // Then — the count, and the ref it was actually measured against (D28)
    let base_sync = base_sync_of(&resolution);
    assert_eq!(
        (
            base_sync.behind_count,
            base_sync.ahead_count,
            base_sync.has_conflicts,
            base_sync.unavailable,
            base_sync.base_branch.as_str()
        ),
        (1, 1, false, false, "origin/master")
    );
}

#[tokio::test]
async fn reports_the_conflicting_paths_of_a_branch_that_cannot_take_its_base() {
    // Given — the branch and master have each rewritten the same file
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    a_repo_with_a_github_origin(&repo);
    a_branch(&repo, BRANCH, true);
    a_commit_on_master_touching(&repo, "g", "the base rewrote this");
    let sessions_base = temp.path().join("data");
    a_pr_stack_orchestrator_with_a_child_on(&sessions_base, &repo);

    // When
    let resolution = query_against(&a_service(sessions_base, false), BRANCH, "origin/master").await;

    // Then — the operator learns which files stand in the way, not merely that something does
    let base_sync = base_sync_of(&resolution);
    assert_eq!(
        (base_sync.has_conflicts, base_sync.conflicted_paths.clone()),
        (true, vec!["g".to_string()])
    );
}

#[tokio::test]
async fn reports_a_branch_that_contains_every_commit_on_its_base_as_neither_behind_nor_conflicting()
{
    // Given — the branch forked and the base has not moved since
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    a_repo_with_a_github_origin(&repo);
    a_branch(&repo, BRANCH, true);
    let sessions_base = temp.path().join("data");
    a_pr_stack_orchestrator_with_a_child_on(&sessions_base, &repo);

    // When
    let resolution = query_against(&a_service(sessions_base, false), BRANCH, "origin/master").await;

    // Then — a healthy comparison, explicitly *not* flagged unavailable
    let base_sync = base_sync_of(&resolution);
    assert_eq!(
        (
            base_sync.behind_count,
            base_sync.has_conflicts,
            base_sync.unavailable
        ),
        (0, false, false)
    );
}

#[tokio::test]
async fn an_unnamed_base_is_reported_unavailable_rather_than_substituted_with_the_default() {
    // Given — a caller with no base to name, e.g. a project that stores no default branch
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    a_repo_with_a_github_origin(&repo);
    a_branch(&repo, BRANCH, true);
    let sessions_base = temp.path().join("data");
    a_pr_stack_orchestrator_with_a_child_on(&sessions_base, &repo);

    // When
    let resolution = query(&a_service(sessions_base, false), BRANCH).await;

    // Then — substituting a base would answer a question the row is not asking (D29), and a zeroed
    // comparison would read as "in sync" against a base the row never names
    let base_sync = base_sync_of(&resolution);
    assert!(base_sync.unavailable);
    assert!(
        !base_sync.unavailable_reason.trim().is_empty(),
        "unavailability must carry an operator-facing reason"
    );
}

#[tokio::test]
async fn a_base_that_names_no_ref_is_reported_unavailable_rather_than_in_sync() {
    // Given — a base branch that does not exist in this repository
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    a_repo_with_a_github_origin(&repo);
    a_branch(&repo, BRANCH, true);
    let sessions_base = temp.path().join("data");
    a_pr_stack_orchestrator_with_a_child_on(&sessions_base, &repo);

    // When
    let resolution = query_against(
        &a_service(sessions_base, false),
        BRANCH,
        "release/never-created",
    )
    .await;

    // Then — a failed comparison is byte-identical to a healthy one on every other field (D27)
    let base_sync = base_sync_of(&resolution);
    assert_eq!(
        (
            base_sync.unavailable,
            base_sync.behind_count,
            base_sync.has_conflicts
        ),
        (true, 0, false)
    );
    assert!(
        base_sync
            .unavailable_reason
            .contains("release/never-created"),
        "the reason must name the base that could not be resolved, was '{}'",
        base_sync.unavailable_reason
    );
}

#[tokio::test]
async fn a_base_sync_that_cannot_be_computed_leaves_the_other_legs_intact() {
    // Given — a base nothing in the repository resolves, so only the comparison can fail
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    a_repo_with_a_github_origin(&repo);
    a_branch(&repo, BRANCH, true);
    let sessions_base = temp.path().join("data");
    a_pr_stack_orchestrator_with_a_child_on(&sessions_base, &repo);

    // When
    let resolution = query_against(
        &a_service(sessions_base, false),
        BRANCH,
        "release/never-created",
    )
    .await;

    // Then — the failure degrades itself and nothing else (AC 23)
    assert!(base_sync_of(&resolution).unavailable);
    let session = resolution.session.expect("the session leg must be present");
    let worktree = resolution
        .worktree
        .expect("the worktree leg must be present");
    let remote = resolution.remote.expect("the remote leg must be present");
    let pr = resolution.pr.expect("the pr leg must be present");
    assert_eq!(
        (
            session.exists,
            session.session_id.as_str(),
            worktree.exists,
            remote.exists,
            pr.unavailable
        ),
        (true, CHILD, false, true, true)
    );
}

#[tokio::test]
async fn reports_a_clean_worktree_as_holding_nothing_outstanding() {
    // Given — a branch checked out in a worktree with no edits in it
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    a_repo_with_a_github_origin(&repo);
    a_branch(&repo, BRANCH, true);
    let worktree = temp.path().join("wt");
    git(
        &repo,
        &["worktree", "add", worktree.to_str().unwrap(), BRANCH],
    );
    let sessions_base = temp.path().join("data");
    a_pr_stack_orchestrator_with_a_child_on(&sessions_base, &repo);

    // When
    let resolution = query(&a_service(sessions_base, false), BRANCH).await;

    // Then
    let leg = resolution
        .worktree
        .expect("the worktree leg must be present");
    assert_eq!((leg.exists, leg.dirty), (true, false));
}

#[tokio::test]
async fn reports_the_tracked_paths_a_worktree_has_left_uncommitted() {
    // Given — an edit to a tracked file, left uncommitted, beside a file git has never seen
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    a_repo_with_a_github_origin(&repo);
    a_branch(&repo, BRANCH, true);
    let worktree = temp.path().join("wt");
    git(
        &repo,
        &["worktree", "add", worktree.to_str().unwrap(), BRANCH],
    );
    std::fs::write(worktree.join("g"), "an unsaved edit").unwrap();
    std::fs::write(worktree.join("scratch.md"), "notes to self").unwrap();
    let sessions_base = temp.path().join("data");
    a_pr_stack_orchestrator_with_a_child_on(&sessions_base, &repo);

    // When
    let resolution = query(&a_service(sessions_base, false), BRANCH).await;

    // Then — the untracked file is not outstanding work: git refuses loudly rather than clobbering
    // one, and counting it would leave the pull control dead in every real agent worktree
    let leg = resolution
        .worktree
        .expect("the worktree leg must be present");
    assert_eq!(
        (leg.dirty, leg.dirty_paths.clone()),
        (true, vec!["g".to_string()])
    );
}
