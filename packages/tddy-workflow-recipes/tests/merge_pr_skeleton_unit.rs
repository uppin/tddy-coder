//! Granular RED-phase tests: production skeleton entry points return **errors** until git/GitHub IO
//! is implemented (Green). Assertions match current skeleton behavior; flip to `is_ok()` / stricter
//! hooks checks when implementing Green.

use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_workflow_recipes::merge_pr::git_ops::{self, MergePrGitConfig};
use tddy_workflow_recipes::merge_pr::github::{self, MergePrGithubParams};
use tddy_workflow_recipes::merge_pr::MergePrWorkflowHooks;

const RED_SYNC_MSG: &str = "merge-pr RED skeleton: sync_feature_with_origin_main not implemented";
const RED_UNMERGED_MSG: &str = "merge-pr RED skeleton: ensure_no_unmerged_paths not implemented";
const RED_GITHUB_MSG: &str = "merge-pr RED skeleton: GitHub merge not implemented";

#[test]
fn merge_pr_hooks_before_task_ok_for_sync_main_red() {
    let h = MergePrWorkflowHooks::new(None);
    h.before_task("sync-main", &Context::new())
        .expect("before_task");
    // Green: assert origin/main context injection for sync-main (see recipe hooks).
}

#[test]
fn merge_pr_git_sync_reports_red_skeleton_until_implementation() {
    let r = git_ops::sync_feature_with_origin_main(&MergePrGitConfig::default());
    assert_eq!(r.unwrap_err(), RED_SYNC_MSG);
}

#[test]
fn merge_pr_git_rejects_unmerged_index_red_skeleton() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let r = git_ops::ensure_no_unmerged_paths(tmp.path());
    assert_eq!(r.unwrap_err(), RED_UNMERGED_MSG);
}

#[test]
fn merge_pr_github_merge_returns_red_skeleton_until_implementation() {
    let r = github::merge_open_pr_for_branch(MergePrGithubParams::default());
    assert_eq!(r.unwrap_err(), RED_GITHUB_MSG);
}

#[test]
fn merge_pr_git_sync_red_skeleton_with_session_worktree_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = MergePrGitConfig {
        session_worktree: Some(tmp.path().to_path_buf()),
    };
    let r = git_ops::sync_feature_with_origin_main(&cfg);
    assert_eq!(r.unwrap_err(), RED_SYNC_MSG);
}
