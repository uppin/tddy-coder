//! Acceptance: after `analyze` submit, `changeset.yaml` reflects branch/worktree (and name when set).

use tddy_core::changeset::{
    read_changeset, write_changeset, BranchWorktreeIntent, Changeset, ChangesetWorkflow,
};
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::task::{NextAction, TaskResult};
use tddy_workflow_recipes::bugfix::BugfixWorkflowHooks;

#[test]
fn bugfix_analyze_persists_branch_and_worktree() {
    let dir = std::env::temp_dir().join(format!("bugfix-analyze-persist-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp session dir");

    write_changeset(&dir, &Changeset::default()).expect("seed changeset.yaml");

    let hooks = BugfixWorkflowHooks::new(None);
    let ctx = Context::new();
    ctx.set_sync("session_dir", dir.clone());

    let result = TaskResult {
        response: r#"{"goal":"analyze","branch_suggestion":"bugfix/test-branch","worktree_suggestion":"bugfix-test-branch","name":"Test bugfix"}"#
            .to_string(),
        next_action: NextAction::Continue,
        task_id: "analyze".to_string(),
        status_message: None,
    };

    hooks
        .after_task("analyze", &ctx, &result)
        .expect("after_task hook");

    let cs = read_changeset(&dir).expect("read changeset after analyze");
    assert!(
        cs.branch_suggestion.is_some() && cs.worktree_suggestion.is_some(),
        "analyze submit must persist branch_suggestion and worktree_suggestion onto changeset; got {:?}",
        cs
    );
}

#[test]
fn bugfix_analyze_sets_workflow_new_branch_name_when_intent_is_new_branch_from_base() {
    let dir = std::env::temp_dir().join(format!("bugfix-analyze-nb-intent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp session dir");

    let cs = Changeset {
        workflow: Some(ChangesetWorkflow {
            branch_worktree_intent: Some(BranchWorktreeIntent::NewBranchFromBase),
            selected_integration_base_ref: Some("origin/main".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    write_changeset(&dir, &cs).expect("seed changeset.yaml");

    let hooks = BugfixWorkflowHooks::new(None);
    let ctx = Context::new();
    ctx.set_sync("session_dir", dir.clone());

    let result = TaskResult {
        response: r#"{"goal":"analyze","branch_suggestion":"bugfix/oauth-crash","worktree_suggestion":"bugfix-oauth-crash"}"#
            .to_string(),
        next_action: NextAction::Continue,
        task_id: "analyze".to_string(),
        status_message: None,
    };

    hooks
        .after_task("analyze", &ctx, &result)
        .expect("after_task hook");

    let cs = read_changeset(&dir).expect("read changeset after analyze");
    assert_eq!(
        cs.workflow
            .as_ref()
            .and_then(|w| w.new_branch_name.as_deref()),
        Some("bugfix/oauth-crash"),
        "LLM branch_suggestion must populate workflow.new_branch_name for new_branch_from_base"
    );
}
