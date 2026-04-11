//! PlanTask retries when the agent finishes a turn without relayed `tddy-tools submit`.

use std::sync::Arc;

use tddy_core::backend::MockBackend;
use tddy_core::error::WorkflowError;
use tddy_core::output::create_session_dir_in;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::task::{NextAction, Task};
use tddy_workflow_recipes::{PlanTask, PlanningOutput, TddRecipe};

const VALID_PLAN_JSON: &str = r##"{"goal":"plan","prd":"# PRD\n\n## TODO\n\n- [ ] Task"}"##;

#[tokio::test]
async fn plan_task_retries_with_remediation_then_succeeds_on_second_invoke() {
    let output_dir =
        std::env::temp_dir().join(format!("tddy-plan-retry-ok-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create output dir");
    let session_dir = create_session_dir_in(&output_dir).expect("pre-create session dir");

    let backend = Arc::new(MockBackend::new());
    backend.push_ok_without_submit("agent finished without submit");
    backend.push_ok(VALID_PLAN_JSON);

    let task = PlanTask::new(backend.clone(), Arc::new(TddRecipe));

    let ctx = Context::new();
    ctx.set_sync("feature_input", "Feature X SKIP_QUESTIONS");
    ctx.set_sync("output_dir", output_dir.clone());
    ctx.set_sync("session_dir", session_dir.clone());

    let result = task
        .run(ctx.clone())
        .await
        .expect("PlanTask should succeed");
    assert_eq!(result.task_id, "plan");
    assert!(matches!(result.next_action, NextAction::Continue));
    assert!(ctx.get_sync::<PlanningOutput>("parsed_planning").is_some());

    let invocations = backend.invocations();
    assert_eq!(
        invocations.len(),
        2,
        "expected one retry after missing submit"
    );
    assert!(
        invocations[1].prompt.contains("createPlan"),
        "second prompt should include remediation; got len {}",
        invocations[1].prompt.len()
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

#[tokio::test]
async fn plan_task_fails_after_eight_invokes_without_submit() {
    let output_dir =
        std::env::temp_dir().join(format!("tddy-plan-retry-fail-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create output dir");
    let session_dir = create_session_dir_in(&output_dir).expect("pre-create session dir");

    let backend = Arc::new(MockBackend::new());
    for _ in 0..8 {
        backend.push_ok_without_submit("still no submit");
    }

    let task = PlanTask::new(backend.clone(), Arc::new(TddRecipe));

    let ctx = Context::new();
    ctx.set_sync("feature_input", "Feature Y SKIP_QUESTIONS");
    ctx.set_sync("output_dir", output_dir.clone());
    ctx.set_sync("session_dir", session_dir.clone());

    let err = task
        .run(ctx)
        .await
        .expect_err("PlanTask should fail after max attempts");
    let inner = err.downcast_ref::<WorkflowError>().expect("WorkflowError");
    let msg = format!("{}", inner);
    assert!(
        msg.contains("8 attempts") || msg.contains("tddy-tools submit"),
        "unexpected error: {msg}"
    );

    assert_eq!(
        backend.invocations().len(),
        8,
        "should exhaust retry budget"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}
