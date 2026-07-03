//! PRD acceptance: writing the stack plan seeds `Changeset.stack` in the same turn.
//!
//! With the `orchestrate` free-prompting loop there is no `begin-orchestrate` task to seed the
//! stack later — the `write-stack-plan` hook must populate `Changeset.stack` from the plan
//! immediately, otherwise the orchestrate goal and its `pr_*` tools operate on an empty stack.
//!
//! PRD: docs/ft/coder/pr-stacking.md § pr-stack recipe.

use tddy_core::changeset::{read_changeset, write_changeset, Changeset};
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::task::{NextAction, TaskResult};
use tddy_workflow_recipes::pr_stack::PrStackHooks;

const A_VALID_PLAN: &str = r#"
version: 1
prs:
  - node_id: n1
    title: Token store
    description: ""
    branch_suggestion: feature/auth/token-store
    parents: []
  - node_id: n2
    title: Login API
    description: ""
    branch_suggestion: feature/auth/login-api
    parents: [n1]
"#;

#[test]
fn writing_the_stack_plan_seeds_the_changeset_stack_on_the_first_write() {
    // Given — a fresh orchestrator session whose changeset has no stack yet
    let dir = std::env::temp_dir().join(format!("pr-stack-seed-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp session dir");
    write_changeset(&dir, &Changeset::default()).expect("seed changeset.yaml");
    assert!(
        read_changeset(&dir).unwrap().stack.is_none(),
        "precondition: no stack before the plan is written"
    );

    let hooks = PrStackHooks::new(None);
    let ctx = Context::new();
    ctx.set_sync("session_dir", dir.clone());
    ctx.set_sync("output", A_VALID_PLAN.to_string());
    let result = TaskResult {
        response: A_VALID_PLAN.to_string(),
        next_action: NextAction::Continue,
        task_id: "write-stack-plan".to_string(),
        status_message: None,
    };

    // When
    hooks
        .after_task("write-stack-plan", &ctx, &result)
        .expect("write-stack-plan after_task hook");

    // Then — the stack is populated from the plan, ready for the orchestrate loop's tools
    let stack = read_changeset(&dir)
        .expect("read changeset")
        .stack
        .expect("writing the plan must seed Changeset.stack");
    let _ = std::fs::remove_dir_all(&dir);
    let node_ids: Vec<&str> = stack.nodes.iter().map(|n| n.node_id.as_str()).collect();
    assert_eq!(node_ids, vec!["n1", "n2"]);
}
