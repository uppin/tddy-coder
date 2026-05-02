//! RED granular tests: workflow context merge and routing keys for branch/worktree intent (PRD).

use std::fs;

use tddy_core::changeset::{
    merge_persisted_workflow_into_context, write_changeset, BranchWorktreeIntent, Changeset,
    ChangesetWorkflow,
};
use tddy_core::workflow::context::Context;

fn temp_dir(label: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "tddy-bw-intent-red-{}-{}",
        label,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

/// GREEN must call `Context::set_sync` for persisted intent so hooks read a single source of truth.
#[test]
fn merge_persisted_workflow_sets_branch_worktree_intent_context_key() {
    let dir = temp_dir("ctx");
    let cs = Changeset {
        workflow: Some(ChangesetWorkflow {
            run_optional_step_x: Some(false),
            demo_options: vec![],
            tool_schema_id: Some("urn:tddy:tool/changeset-workflow".into()),
            branch_worktree_intent: Some(BranchWorktreeIntent::NewBranchFromBase),
            selected_integration_base_ref: Some("origin/main".into()),
            new_branch_name: Some("feature/x".into()),
            selected_branch_to_work_on: None,
            ..Default::default()
        }),
        ..Default::default()
    };
    write_changeset(&dir, &cs).unwrap();

    let ctx = Context::new();
    merge_persisted_workflow_into_context(&dir, &ctx).unwrap();

    assert!(
        ctx.get_sync::<String>("branch_worktree_intent").is_some(),
        "GREEN: merge must expose branch_worktree_intent on Context for resume hooks"
    );

    let _ = fs::remove_dir_all(&dir);
}
