//! Regression: bugfix `reproduce` must run [`ensure_worktree_for_session`] (same as TDD acceptance-tests)
//! so `worktree_dir` is set and a git worktree exists when the analyze step has populated branch/worktree names.

use std::fs;
use std::path::PathBuf;

use tddy_core::changeset::{read_changeset, write_changeset, Changeset, ChangesetState};
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::ids::WorkflowState;
use tddy_workflow_recipes::bugfix::BugfixWorkflowHooks;

fn temp_dir(label: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("bugfix-repro-wt-{}-{}", label, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn bugfix_reproduce_stub_sets_worktree_dir_from_output_dir() {
    let base = temp_dir("stub");
    let session_dir = base.join("session");
    fs::create_dir_all(&session_dir).unwrap();
    let cs = Changeset {
        recipe: Some("bugfix".to_string()),
        branch_suggestion: Some("bugfix/example".into()),
        worktree_suggestion: Some("bugfix-example".into()),
        state: ChangesetState {
            current: WorkflowState::new("Reproducing"),
            ..Changeset::default().state
        },
        ..Default::default()
    };
    write_changeset(&session_dir, &cs).unwrap();

    let hooks = BugfixWorkflowHooks::new(None);
    let ctx = Context::new();
    ctx.set_sync("session_dir", session_dir.clone());
    ctx.set_sync("output_dir", base.clone());
    ctx.set_sync("backend_name", "stub".to_string());

    hooks
        .before_task("reproduce", &ctx)
        .expect("reproduce before_task should succeed with stub backend");

    let wt: PathBuf = ctx
        .get_sync("worktree_dir")
        .expect("reproduce hook must set worktree_dir (stub uses output_dir)");
    assert_eq!(wt, base);
}

#[test]
fn bugfix_reproduce_creates_git_worktree_when_backend_not_stub() {
    let base = temp_dir("git");
    let repo = base.join("repo");
    fs::create_dir_all(&repo).unwrap();

    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(&repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(&repo)
        .output()
        .unwrap();
    fs::write(repo.join("README"), "initial").unwrap();
    std::process::Command::new("git")
        .args(["add", "README"])
        .current_dir(&repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(&repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["branch", "-M", "master"])
        .current_dir(&repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["remote", "add", "origin", repo.to_str().unwrap()])
        .current_dir(&repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["push", "-u", "origin", "master"])
        .current_dir(&repo)
        .output()
        .unwrap();

    let session_dir = base.join("plan");
    fs::create_dir_all(&session_dir).unwrap();
    let cs = Changeset {
        recipe: Some("bugfix".to_string()),
        branch_suggestion: Some("bugfix/repro-wt".into()),
        worktree_suggestion: Some("bugfix-repro-wt".into()),
        state: ChangesetState {
            current: WorkflowState::new("Reproducing"),
            ..Changeset::default().state
        },
        ..Default::default()
    };
    write_changeset(&session_dir, &cs).unwrap();

    let hooks = BugfixWorkflowHooks::new(None);
    let ctx = Context::new();
    ctx.set_sync("session_dir", session_dir.clone());
    ctx.set_sync("output_dir", repo.clone());
    ctx.set_sync("backend_name", "claude".to_string());

    hooks
        .before_task("reproduce", &ctx)
        .expect("reproduce before_task should create worktree");

    let wt: PathBuf = ctx
        .get_sync("worktree_dir")
        .expect("reproduce hook must set worktree_dir");
    assert!(wt.exists(), "worktree path should exist: {}", wt.display());

    let cs_after = read_changeset(&session_dir).expect("read changeset");
    assert!(
        cs_after.worktree.is_some(),
        "changeset should persist worktree path after setup"
    );
}
