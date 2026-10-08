//! Unit tests for merge-pr git helpers, GitHub merge entry point, and hooks.

use tddy_core::changeset::{write_changeset, BranchWorktreeIntent, Changeset, ChangesetWorkflow};
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_workflow_recipes::merge_pr::git_ops::{self, MergePrGitConfig};
use tddy_workflow_recipes::merge_pr::github::{self, MergePrGithubParams};
use tddy_workflow_recipes::merge_pr::MergePrWorkflowHooks;

const RED_SYNC_MSG: &str = "merge-pr RED skeleton: sync_feature_with_origin_main not implemented";

#[test]
fn merge_pr_hooks_before_task_ok_for_analyze() {
    // Given
    let h = MergePrWorkflowHooks::new(None);
    let ctx = Context::new();

    // When
    h.before_task("analyze", &ctx).expect("before_task");

    // Then
    let prompt = ctx
        .get_sync::<String>("system_prompt")
        .expect("system_prompt");
    assert!(
        prompt.contains("read-only") || prompt.contains("Read-only"),
        "analyze prompt should mention read-only; got: {prompt}"
    );
    assert!(
        prompt.contains("conflict"),
        "analyze prompt should mention conflicts; got: {prompt}"
    );
}

#[test]
fn merge_pr_hooks_before_task_ok_for_sync_main_no_worktree() {
    // Given
    let h = MergePrWorkflowHooks::new(None);
    let ctx = Context::new();

    // When — sync-main without session_dir skips worktree setup but still sets prompt
    h.before_task("sync-main", &ctx).expect("before_task");

    // Then
    assert!(
        ctx.get_sync::<String>("system_prompt")
            .map(|p| p.contains("merge-pr") && p.contains("Merge"))
            .unwrap_or(false),
        "sync-main should set a merge-pr system prompt with task instructions"
    );
}

#[test]
fn merge_pr_git_sync_errors_without_session_worktree() {
    // When
    let r = git_ops::sync_feature_with_origin_main(&MergePrGitConfig::default());
    let err = r.unwrap_err();

    // Then
    assert!(
        !err.contains(RED_SYNC_MSG),
        "must not use RED skeleton message; got {err}"
    );
    assert!(
        err.contains("session worktree") || err.contains("worktree"),
        "expected session worktree error; got {err}"
    );
}

#[test]
fn merge_pr_git_clean_tempdir_has_no_unmerged_paths() {
    // Given
    let tmp = tempfile::tempdir().expect("tempdir");

    // When / Then — must not error
    git_ops::ensure_no_unmerged_paths(tmp.path()).expect("no unmerged paths in non-repo");
}

#[test]
fn merge_pr_github_merge_errors_when_token_missing() {
    // Given no token is passed
    let token = "";

    // When
    let r = github::merge_open_pr_for_branch(MergePrGithubParams::default(), token);
    let err = r.unwrap_err();

    // Then
    assert!(
        err.contains("credential"),
        "expected missing credential message; got {err}"
    );
}

#[test]
fn merge_pr_git_sync_errors_when_path_not_a_git_repo() {
    // Given
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = MergePrGitConfig {
        session_worktree: Some(tmp.path().to_path_buf()),
        ..Default::default()
    };

    // When
    let r = git_ops::sync_feature_with_origin_main(&cfg);
    let err = r.unwrap_err();

    // Then
    assert!(
        !err.contains(RED_SYNC_MSG),
        "must not use RED skeleton; got {err}"
    );
    assert!(
        err.contains("not a git repository") || err.contains(".git"),
        "expected not-a-repo error; got {err}"
    );
}

fn git_run(args: &[&str], cwd: &std::path::Path) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("git {:?} in {}: {e}", args, cwd.display()));
    assert!(
        out.status.success(),
        "git {:?} failed in {}:\n{}",
        args,
        cwd.display(),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// When the changeset has `branch_worktree_intent: work_on_selected_branch` with
/// `selected_branch_to_work_on: feature/other`, the analyze step must incorporate
/// the intended branch into its context — not just use whatever branch `output_dir`
/// happens to be on.
#[test]
fn merge_pr_analyze_reads_changeset_branch_intent() {
    // Given
    let tmp = tempfile::tempdir().expect("tempdir");
    let bare = tmp.path().join("origin.git");
    let clone = tmp.path().join("work");
    git_run(&["init", "--bare", bare.to_str().unwrap()], tmp.path());
    git_run(&["clone", bare.to_str().unwrap(), "work"], tmp.path());
    git_run(&["config", "user.email", "t@t.com"], &clone);
    git_run(&["config", "user.name", "t"], &clone);
    std::fs::write(clone.join("f.txt"), "base\n").unwrap();
    git_run(&["add", "f.txt"], &clone);
    git_run(&["commit", "-m", "init"], &clone);
    git_run(&["branch", "-M", "master"], &clone);
    git_run(&["push", "-u", "origin", "master"], &clone);
    git_run(&["checkout", "-b", "feature/other"], &clone);
    std::fs::write(clone.join("f.txt"), "other\n").unwrap();
    git_run(&["add", "f.txt"], &clone);
    git_run(&["commit", "-m", "other"], &clone);
    git_run(&["push", "-u", "origin", "feature/other"], &clone);
    git_run(&["checkout", "master"], &clone);
    let session_dir = tmp.path().join("session");
    std::fs::create_dir_all(&session_dir).unwrap();
    let cs = Changeset {
        recipe: Some("merge-pr".to_string()),
        repo_path: Some(clone.display().to_string()),
        workflow: Some(ChangesetWorkflow {
            branch_worktree_intent: Some(BranchWorktreeIntent::WorkOnSelectedBranch),
            selected_branch_to_work_on: Some("feature/other".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    };
    write_changeset(&session_dir, &cs).expect("write changeset");
    let h = MergePrWorkflowHooks::new(None);
    let ctx = Context::new();
    ctx.set_sync("output_dir", clone.clone());
    ctx.set_sync("session_dir", session_dir);

    // When
    h.before_task("analyze", &ctx).expect("before_task");

    // Then
    let prompt = ctx
        .get_sync::<String>("system_prompt")
        .expect("system_prompt must be set");
    assert!(
        prompt.contains("feature/other"),
        "analyze system prompt must reference the intended branch from changeset \
         (feature/other), not the currently checked-out branch (master); got:\n{prompt}"
    );
}

#[test]
fn merge_pr_prompt_advertises_the_github_pr_tools_when_the_session_can_authenticate_them() {
    // Given a session whose host answers the tools' token requests
    let h = MergePrWorkflowHooks::new(None);
    let ctx = Context::new();
    ctx.set_sync(
        tddy_workflow_recipes::github_pr_tools::GITHUB_PR_TOOLS_AVAILABLE_KEY,
        true,
    );

    // When the analyze task is prepared
    h.before_task("analyze", &ctx).expect("before_task");

    // Then the prompt carries the awareness line
    let prompt = ctx
        .get_sync::<String>("system_prompt")
        .expect("system_prompt");
    assert!(
        prompt
            .contains(tddy_workflow_recipes::merge_pr::merge_pr_github_tools_awareness_line(true)),
        "the PR tools awareness must be appended; got: {prompt}"
    );
}

#[test]
fn merge_pr_prompt_does_not_advertise_the_github_pr_tools_without_a_credential_to_ask_for() {
    // Given a session whose context says nothing about a credential
    let h = MergePrWorkflowHooks::new(None);
    let ctx = Context::new();

    // When the analyze task is prepared
    h.before_task("analyze", &ctx).expect("before_task");

    // Then the prompt makes no promise of PR tools
    let prompt = ctx
        .get_sync::<String>("system_prompt")
        .expect("system_prompt");
    assert!(
        !prompt.contains("github_create_pull_request"),
        "no awareness without a credential; got: {prompt}"
    );
}
