//! Acceptance tests for worktrees listing, cached stats, and removal (library API; no ConnectionService RPC yet).

use std::path::Path;
use std::process::Command;
use std::sync::atomic::Ordering;

use tddy_daemon::worktrees::{
    projects_stats_cache_root, remove_worktree_under_repo, WorktreeStatsCache,
};

fn require_git() {
    let ok = Command::new("git")
        .arg("--version")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    assert!(ok, "git must be available for worktree acceptance tests");
}

fn init_repo_with_secondary_worktree() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf)
{
    require_git();
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("main");
    std::fs::create_dir_all(&repo).unwrap();
    run_git(&repo, &["init"]);
    run_git(&repo, &["config", "user.email", "t@e.st"]);
    run_git(&repo, &["config", "user.name", "t"]);
    std::fs::write(repo.join("README.md"), "x\n").unwrap();
    run_git(&repo, &["add", "README.md"]);
    run_git(&repo, &["commit", "-m", "init"]);
    let wt = tmp.path().join("wt-secondary");
    run_git(
        &repo,
        &[
            "worktree",
            "add",
            wt.to_str().unwrap(),
            "-b",
            "acceptance-branch",
        ],
    );
    (tmp, repo, wt)
}

fn run_git(cwd: &Path, args: &[&str]) {
    let st = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("git {:?} in {:?}: {e}", args, cwd));
    assert!(st.success(), "git {:?} failed in {:?}", args, cwd);
}

fn worktree_list_contains_path(repo: &Path, needle: &Path) -> bool {
    let out = Command::new("git")
        .current_dir(repo)
        .args(["worktree", "list"])
        .output()
        .expect("git worktree list");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout
        .lines()
        .any(|line| line.split_whitespace().next() == Some(needle.to_str().unwrap()))
}

/// Acceptance: after first refresh, repeated list does not invoke diff/stat again (counter).
#[test]
fn stats_cache_persists_and_is_served_without_re_diff_on_each_list_call() {
    let tmp = tempfile::tempdir().unwrap();
    let prev = std::env::var("TDDY_PROJECTS_STATS_ROOT").ok();
    std::env::set_var(
        "TDDY_PROJECTS_STATS_ROOT",
        tmp.path().to_str().expect("utf8 temp path"),
    );
    let _restore = scopeguard::guard(prev, |p| {
        if let Some(v) = p {
            std::env::set_var("TDDY_PROJECTS_STATS_ROOT", v);
        } else {
            std::env::remove_var("TDDY_PROJECTS_STATS_ROOT");
        }
    });
    let root = projects_stats_cache_root();
    assert_eq!(root, tmp.path());

    let cache = WorktreeStatsCache::new(root);
    let project_id = "proj-acceptance-1";
    let (_guard, main_repo, _wt) = init_repo_with_secondary_worktree();

    cache.refresh_stats_for_project(project_id, &main_repo);
    let after_refresh = cache.test_git_diff_invocations.load(Ordering::SeqCst);
    assert_eq!(after_refresh, 1);

    cache.list_cached_stats(project_id);
    cache.list_cached_stats(project_id);

    assert_eq!(
        cache.test_git_diff_invocations.load(Ordering::SeqCst),
        1,
        "list must not schedule additional refresh/diff work"
    );
}

/// Acceptance: successful remove drops worktree from `git worktree list`; repeat remove fails.
#[test]
fn remove_worktree_drops_listing_and_repeat_fails() {
    let (_tmp, repo, wt_path) = init_repo_with_secondary_worktree();
    assert!(
        worktree_list_contains_path(&repo, &wt_path),
        "precondition: secondary worktree must exist"
    );

    let res = remove_worktree_under_repo(&repo, &wt_path);
    assert!(res.is_ok(), "remove worktree should succeed: {:?}", res);

    assert!(
        !worktree_list_contains_path(&repo, &wt_path),
        "git worktree list should no longer list removed path"
    );

    let second = remove_worktree_under_repo(&repo, &wt_path);
    assert!(
        second.is_err(),
        "second delete should be not-found / failed precondition"
    );
}
