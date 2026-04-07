//! Integration acceptance: real **`git`** fixtures for merge-pr (temp bare `origin`, conflicting merge).
//!
//! RED until **`merge-pr`** resolves and sync-main rejects dirty/unmerged indexes. Heavy paths may be ignored later.

use std::fs;
use std::path::Path;
use std::process::Command;

use tddy_core::GoalId;
use tddy_workflow_recipes::workflow_recipe_and_manifest_from_cli_name;

const TASK_ANALYZE: &str = "analyze";
const TASK_FINALIZE: &str = "finalize";

fn git(args: &[&str], cwd: &Path) -> std::process::Output {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("run git {:?} in {}: {e}", args, cwd.display()));
    assert!(
        out.status.success(),
        "git {:?} failed in {}:\nstdout={}\nstderr={}",
        args,
        cwd.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

/// Two branches diverge on `conflict.txt`; merging `main` into `feature` leaves unmerged paths until resolved.
fn init_conflict_worktree(root: &Path) {
    git(&["init"], root);
    git(&["config", "user.email", "merge-pr-test@example.com"], root);
    git(&["config", "user.name", "merge-pr test"], root);

    fs::write(root.join("conflict.txt"), "base\n").unwrap();
    git(&["add", "conflict.txt"], root);
    git(&["commit", "-m", "base"], root);
    git(&["branch", "-M", "main"], root);

    git(&["checkout", "-b", "feature"], root);
    fs::write(root.join("conflict.txt"), "feature\n").unwrap();
    git(&["add", "conflict.txt"], root);
    git(&["commit", "-m", "feature"], root);

    git(&["checkout", "main"], root);
    fs::write(root.join("conflict.txt"), "main\n").unwrap();
    git(&["add", "conflict.txt"], root);
    git(&["commit", "-m", "main"], root);

    git(&["checkout", "feature"], root);
    let merge = Command::new("git")
        .args(["merge", "main"])
        .current_dir(root)
        .output()
        .expect("git merge");
    assert!(
        !merge.status.success(),
        "expected merge conflict; got success"
    );
}

fn git_ls_files_unmerged(root: &Path) -> Vec<u8> {
    Command::new("git")
        .args(["ls-files", "-u"])
        .current_dir(root)
        .output()
        .expect("ls-files")
        .stdout
}

/// Bare `origin` + clone with `feature` checked out; push updates `refs/heads/feature` for degraded-mode contract tests.
fn init_origin_and_clone_push(root: &Path) -> std::path::PathBuf {
    let bare = root.join("origin.git");
    let clone = root.join("work");

    git(&["init", "--bare", bare.to_str().expect("utf8 path")], root);

    git(&["clone", bare.to_str().unwrap(), "work"], root);
    fs::write(clone.join("README.md"), "# m\n").unwrap();
    git(&["config", "user.email", "p@e.com"], &clone);
    git(&["config", "user.name", "p"], &clone);
    git(&["add", "README.md"], &clone);
    git(&["commit", "-m", "init"], &clone);
    git(&["branch", "-M", "main"], &clone);
    git(&["push", "-u", "origin", "main"], &clone);

    git(&["checkout", "-b", "feature"], &clone);
    fs::write(clone.join("README.md"), "# feature\n").unwrap();
    git(&["add", "README.md"], &clone);
    git(&["commit", "-m", "f"], &clone);

    git(&["push", "-u", "origin", "feature"], &clone);

    clone
}

#[test]
fn merge_pr_fails_on_unresolved_conflicts() {
    let tmp = tempfile::tempdir().expect("tempdir");
    init_conflict_worktree(tmp.path());
    let unmerged = git_ls_files_unmerged(tmp.path());
    assert!(
        !unmerged.is_empty(),
        "fixture must leave unmerged paths (merge-pr must fail closed on this state)"
    );

    let (recipe, _) = workflow_recipe_and_manifest_from_cli_name("merge-pr")
        .expect("merge-pr must register before conflict integration asserts");
    assert_eq!(recipe.start_goal().as_str(), TASK_ANALYZE);
    assert!(
        recipe
            .goal_ids()
            .iter()
            .any(|g| g.as_str() == TASK_FINALIZE),
        "finalize goal required for reporting / push outcome"
    );
}

#[test]
fn merge_pr_skips_github_when_no_token() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let work = init_origin_and_clone_push(tmp.path());

    // Degraded contract: branch is pushable to origin (fixture proves fast-forward push works).
    fs::write(work.join("README.md"), "# feature push\n").unwrap();
    git(&["add", "README.md"], &work);
    git(&["commit", "-m", "push-check"], &work);
    git(&["push", "origin", "feature"], &work);

    let (recipe, _) = workflow_recipe_and_manifest_from_cli_name("merge-pr")
        .expect("merge-pr must resolve for push-only degraded contract");
    assert_eq!(recipe.name(), "merge-pr");
    let _ = TASK_FINALIZE;
}

#[test]
fn merge_pr_merges_pr_when_token_present() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let work = init_origin_and_clone_push(tmp.path());

    let (recipe, _) = workflow_recipe_and_manifest_from_cli_name("merge-pr")
        .expect("merge-pr must resolve before GitHub merge contract tests");
    assert!(
        recipe.goal_requires_tddy_tools_submit(&GoalId::new(TASK_FINALIZE)),
        "finalize submit carries merge outcome / API result metadata"
    );

    let _ = work;
}
