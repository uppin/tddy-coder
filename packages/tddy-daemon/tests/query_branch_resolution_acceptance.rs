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
//! PRD: docs/ft/coder/pr-stack-live-status.md (C2, C3, D4, D8, D12).

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
    BranchResolution, ConnectionService as ConnectionServiceTrait, QueryBranchRequest,
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

async fn query(service: &ConnectionServiceImpl, branch: &str) -> BranchResolution {
    service
        .query_branch(Request::new(QueryBranchRequest {
            session_token: TOKEN.to_string(),
            session_id: ORCHESTRATOR.to_string(),
            branch: branch.to_string(),
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
