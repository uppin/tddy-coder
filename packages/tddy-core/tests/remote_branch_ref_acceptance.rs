//! Acceptance: resolving a branch's remote-tracking ref.
//!
//! A PR-stack child worktree is created from `origin/<base>`, so a base branch that is absent from
//! the remote makes the spawn fail inside `git fetch` — after `StartSession` was already accepted and
//! a session directory written. `remote_branch_ref_sha` is the read the daemon needs to answer
//! "can this branch be based upon?" before offering the spawn at all.
//!
//! It resolves the *remote-tracking* ref (`origin/<branch>`) in the local repo, so it is as fresh as
//! the last fetch — conservative by construction: it can delay a spawn, never permit one that would
//! fail. A missing branch, a local-only branch, and a path that is not a repo are all `None`, never
//! an error: this read sits on a polled display path and must not fail the enclosing RPC.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md (C2, D4).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tddy_core::worktree::remote_branch_ref_sha;

fn scratch(label: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "tddy-remote-branch-ref-{}-{}",
        label,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

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

/// A repo with one commit on `master`, its own path added as `origin`, and `master` pushed.
fn a_repo_with_origin(repo: &Path) {
    fs::create_dir_all(repo).unwrap();
    git(repo, &["init", "--initial-branch=master"]);
    git(repo, &["config", "user.email", "t@t.com"]);
    git(repo, &["config", "user.name", "T"]);
    fs::write(repo.join("f"), "x").unwrap();
    git(repo, &["add", "f"]);
    git(repo, &["commit", "-m", "initial"]);
    git(repo, &["remote", "add", "origin", repo.to_str().unwrap()]);
    git(repo, &["push", "-u", "origin", "master"]);
}

#[test]
fn resolves_the_commit_sha_of_a_branch_that_exists_on_the_remote() {
    // Given — feature/pushed is created and pushed to origin
    let base = scratch("pushed");
    let repo = base.join("repo");
    a_repo_with_origin(&repo);
    git(&repo, &["checkout", "-b", "feature/pushed"]);
    fs::write(repo.join("g"), "y").unwrap();
    git(&repo, &["add", "g"]);
    git(&repo, &["commit", "-m", "on the branch"]);
    git(&repo, &["push", "origin", "feature/pushed"]);
    let expected = git(&repo, &["rev-parse", "feature/pushed"]);

    // When
    let sha = remote_branch_ref_sha(&repo, "feature/pushed");
    let _ = fs::remove_dir_all(&base);

    // Then — the remote ref resolves to the commit the branch was pushed at
    assert_eq!(
        sha,
        Some(expected),
        "a branch pushed to origin must resolve to its commit sha"
    );
}

#[test]
fn resolves_nothing_for_a_branch_that_exists_only_locally() {
    // Given — feature/local is committed but never pushed
    let base = scratch("local-only");
    let repo = base.join("repo");
    a_repo_with_origin(&repo);
    git(&repo, &["checkout", "-b", "feature/local"]);
    fs::write(repo.join("g"), "y").unwrap();
    git(&repo, &["add", "g"]);
    git(&repo, &["commit", "-m", "on the branch"]);

    // When
    let sha = remote_branch_ref_sha(&repo, "feature/local");
    let _ = fs::remove_dir_all(&base);

    // Then — a local branch is not a base a child worktree can be created from
    assert_eq!(
        sha, None,
        "an unpushed local branch must not read as available on the remote"
    );
}

#[test]
fn resolves_nothing_for_a_branch_that_does_not_exist() {
    // Given
    let base = scratch("absent");
    let repo = base.join("repo");
    a_repo_with_origin(&repo);

    // When
    let sha = remote_branch_ref_sha(&repo, "feature/never-created");
    let _ = fs::remove_dir_all(&base);

    // Then
    assert_eq!(
        sha, None,
        "a branch that was never created has no remote ref"
    );
}

#[test]
fn resolves_nothing_without_erroring_when_the_path_is_not_a_git_repository() {
    // Given — a plain directory, not a repo
    let base = scratch("not-a-repo");

    // When
    let sha = remote_branch_ref_sha(&base, "master");
    let _ = fs::remove_dir_all(&base);

    // Then — this read runs on a polled display path; it degrades, it never fails the caller
    assert_eq!(
        sha, None,
        "a non-repository path must yield None rather than an error"
    );
}
