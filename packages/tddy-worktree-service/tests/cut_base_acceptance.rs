//! Acceptance: the base a session's worktree was cut from — its commit, its branch, and how far
//! behind its upstream it is — as a start reports it (`#orchestra` 4/9).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Orchestrator sessions.

use std::path::Path;

use pretty_assertions::assert_eq;
use tddy_worktree_service::base_freshness::{branch_of_worktree, CutBase};

fn git(cwd: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} failed to run: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} in {cwd:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A repository whose `origin/main` is one commit ahead of where `feature` was cut, and a worktree
/// on `feature` (tracking `origin/main`) and one on `local-only` (tracking nothing).
fn a_repo_with_two_cuts() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    String,
) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "init"]);
    let cut_from = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["remote", "add", "origin", repo.to_str().unwrap()]);
    git(&repo, &["push", "-q", "origin", "main"]);
    git(&repo, &["fetch", "-q", "origin"]);
    let tracking = dir.path().join("tracking");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature",
            tracking.to_str().unwrap(),
            "origin/main",
        ],
    );
    let untracked = dir.path().join("untracked");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "local-only",
            untracked.to_str().unwrap(),
            "main",
        ],
    );
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "landed"]);
    git(&repo, &["push", "-q", "origin", "main"]);
    git(&repo, &["fetch", "-q", "origin"]);
    (dir, tracking, untracked, cut_from)
}

#[test]
fn a_cut_reports_its_commit_and_how_far_behind_its_upstream_it_is() {
    // Given a worktree cut from origin/main, which has since moved one commit on
    let (_dir, tracking, _untracked, cut_from) = a_repo_with_two_cuts();

    // When its cut base is read
    let base = CutBase::of_worktree(&tracking).unwrap();

    // Then it names the commit it was cut from and is one behind
    assert_eq!(
        base,
        CutBase {
            commit: cut_from,
            behind: 1
        }
    );
}

#[test]
fn a_branch_with_no_upstream_is_zero_behind() {
    // Given a worktree on a branch that tracks nothing
    let (_dir, _tracking, untracked, cut_from) = a_repo_with_two_cuts();

    // When its cut base is read
    let base = CutBase::of_worktree(&untracked).unwrap();

    // Then it has no remote tip to be behind
    assert_eq!(
        base,
        CutBase {
            commit: cut_from,
            behind: 0
        }
    );
}

#[test]
fn the_cut_names_its_branch() {
    // Given a worktree on `feature`
    let (_dir, tracking, _untracked, _cut_from) = a_repo_with_two_cuts();

    // When its branch is read, then it is `feature`
    assert_eq!(branch_of_worktree(&tracking).unwrap(), "feature");
}
