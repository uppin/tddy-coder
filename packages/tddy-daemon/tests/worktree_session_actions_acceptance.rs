//! Acceptance tests for the session-worktree inspector's Clear action: `clean_worktree_under_repo`
//! runs `git clean -fdx` in a secondary session worktree (library API; membership-gated, primary
//! refused). See docs/ft/web/session-worktree-inspector.md.

use std::path::{Path, PathBuf};
use std::process::Command;

use tddy_daemon::worktrees::{clean_worktree_under_repo, CleanWorktreeError};

fn require_git() {
    let ok = Command::new("git")
        .arg("--version")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    assert!(ok, "git must be available for worktree acceptance tests");
}

fn run_git(cwd: &Path, args: &[&str]) {
    let st = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("git {:?} in {:?}: {e}", args, cwd));
    assert!(st.success(), "git {:?} failed in {:?}", args, cwd);
}

/// A main repo (with a committed `.gitignore` ignoring `target/`) plus one secondary worktree.
fn init_repo_with_secondary_worktree() -> (tempfile::TempDir, PathBuf, PathBuf) {
    require_git();
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("main");
    std::fs::create_dir_all(&repo).unwrap();
    run_git(&repo, &["init"]);
    run_git(&repo, &["config", "user.email", "t@e.st"]);
    run_git(&repo, &["config", "user.name", "t"]);
    std::fs::write(repo.join("README.md"), "tracked\n").unwrap();
    std::fs::write(repo.join(".gitignore"), "target/\n").unwrap();
    run_git(&repo, &["add", "README.md", ".gitignore"]);
    run_git(&repo, &["commit", "-m", "init"]);

    let wt = tmp.path().join("wt-secondary");
    run_git(
        &repo,
        &["worktree", "add", wt.to_str().unwrap(), "-b", "feature-x"],
    );
    let wt = wt
        .canonicalize()
        .expect("secondary worktree dir must exist after git worktree add");
    (tmp, repo, wt)
}

/// Acceptance: Clear drops untracked *and* gitignored files (the `-x`) but keeps tracked files.
#[test]
fn clean_worktree_removes_untracked_and_ignored_files_but_keeps_tracked() {
    // Given a secondary worktree with a tracked file, an untracked file, and an ignored build dir
    let (_guard, repo, wt) = init_repo_with_secondary_worktree();
    let tracked = wt.join("README.md");
    let untracked = wt.join("scratch.txt");
    let ignored = wt.join("target").join("build.o");
    std::fs::create_dir_all(wt.join("target")).unwrap();
    std::fs::write(&untracked, "scratch\n").unwrap();
    std::fs::write(&ignored, "obj\n").unwrap();

    // When the worktree is cleared
    let result = clean_worktree_under_repo(&repo, &wt);

    // Then the clear succeeds and only the tracked file survives
    assert!(result.is_ok(), "clean should succeed: {:?}", result);
    assert!(tracked.exists(), "tracked README.md must survive clean");
    assert!(!untracked.exists(), "untracked scratch.txt must be removed");
    assert!(
        !ignored.exists(),
        "gitignored target/build.o must be removed (git clean -fdx)"
    );
}

/// Acceptance: Clear refuses the primary (first-listed) worktree — only secondary worktrees clear.
#[test]
fn clean_worktree_refuses_the_primary_worktree() {
    // Given a repo whose primary checkout holds an untracked file
    let (_guard, repo, _wt) = init_repo_with_secondary_worktree();
    let repo_canon = repo.canonicalize().unwrap();
    let untracked = repo_canon.join("primary-scratch.txt");
    std::fs::write(&untracked, "keep me\n").unwrap();

    // When clearing the primary worktree is attempted
    let result = clean_worktree_under_repo(&repo_canon, &repo_canon);

    // Then it is refused and the primary's untracked file is untouched
    assert_eq!(result, Err(CleanWorktreeError::CannotCleanPrimary));
    assert!(untracked.exists(), "primary files must not be cleared");
}

/// Acceptance: Clear rejects a path that is not one of the repo's worktrees.
#[test]
fn clean_worktree_rejects_path_not_in_worktree_list() {
    // Given a path that is not a registered worktree of the repo
    let (guard, repo, _wt) = init_repo_with_secondary_worktree();
    let stranger = guard.path().join("not-a-worktree");
    std::fs::create_dir_all(&stranger).unwrap();

    // When clearing that path is attempted
    let result = clean_worktree_under_repo(&repo, &stranger);

    // Then it is rejected as not listed
    assert_eq!(result, Err(CleanWorktreeError::NotListed));
}
