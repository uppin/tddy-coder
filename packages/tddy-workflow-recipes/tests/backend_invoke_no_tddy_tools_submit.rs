//! When a recipe marks a goal as not requiring `tddy-tools submit`, [`BackendInvokeTask`] completes
//! from agent output alone (e.g. Cursor CLI without tool relay).

use std::sync::Arc;

use async_trait::async_trait;
use tddy_core::backend::{CodingBackend, InvokeRequest, InvokeResponse};
use tddy_core::workflow::context::Context;
use tddy_core::workflow::task::{BackendInvokeTask, NextAction, Task};
use tddy_core::{BackendError, GoalId, WorkflowRecipe};
use tddy_workflow_recipes::{FreePromptingRecipe, GrillMeRecipe, TddRecipe};

/// Backend that returns successful invoke output and never supplies `tddy-tools submit`.
struct OutputOnlyBackend;

#[async_trait]
impl CodingBackend for OutputOnlyBackend {
    fn submit_channel(&self) -> Option<&tddy_core::toolcall::SubmitResultChannel> {
        None
    }

    async fn invoke(&self, _request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        Ok(InvokeResponse {
            output: "agent reply without submit".to_string(),
            exit_code: 0,
            session_id: Some("sess-output-only".to_string()),
            questions: vec![],
            raw_stream: None,
            stderr: None,
        })
    }

    fn name(&self) -> &str {
        "output-only-test"
    }
}

#[tokio::test]
async fn grill_me_backend_invoke_waits_for_host_gate_without_relayed_ask_answers() {
    let recipe: Arc<dyn WorkflowRecipe> = Arc::new(GrillMeRecipe);
    let backend: Arc<dyn CodingBackend> = Arc::new(OutputOnlyBackend);
    let task = BackendInvokeTask::from_recipe("grill", GoalId::new("grill"), recipe, backend);

    let ctx = Context::new();
    ctx.set_sync("feature_input", "hello");
    ctx.set_sync("output_dir", std::env::temp_dir());

    let result = task
        .run(ctx)
        .await
        .expect("task should succeed without submit");
    assert_eq!(result.response, "agent reply without submit");
    assert!(matches!(result.next_action, NextAction::WaitForInput));
}

#[tokio::test]
async fn grill_me_backend_invoke_continues_when_grill_ask_answers_file_present() {
    let recipe: Arc<dyn WorkflowRecipe> = Arc::new(GrillMeRecipe);
    let backend: Arc<dyn CodingBackend> = Arc::new(OutputOnlyBackend);
    let task = BackendInvokeTask::from_recipe("grill", GoalId::new("grill"), recipe, backend);

    let tmp = std::env::temp_dir().join(format!("grill-gate-relay-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join(".workflow")).unwrap();
    std::fs::write(
        tmp.join(".workflow").join("grill_ask_answers.txt"),
        "from tddy-tools ask relay",
    )
    .unwrap();

    let ctx = Context::new();
    ctx.set_sync("feature_input", "hello");
    ctx.set_sync("output_dir", std::env::temp_dir());
    ctx.set_sync("session_dir", tmp.clone());

    let result = task
        .run(ctx)
        .await
        .expect("task should succeed without submit");
    assert_eq!(result.response, "agent reply without submit");
    assert!(matches!(result.next_action, NextAction::Continue));
    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn tdd_interview_backend_invoke_completes_without_tddy_tools_submit() {
    let recipe = TddRecipe;
    let backend: Arc<dyn CodingBackend> = Arc::new(OutputOnlyBackend);
    let task =
        BackendInvokeTask::from_recipe("interview", GoalId::new("interview"), Arc::new(recipe), backend);

    let ctx = Context::new();
    ctx.set_sync("feature_input", "hello");
    ctx.set_sync("prompt", "elicitation turn");
    ctx.set_sync("output_dir", std::env::temp_dir());

    let result = task
        .run(ctx)
        .await
        .expect("interview task should succeed without submit");
    assert_eq!(result.response, "agent reply without submit");
    assert!(matches!(result.next_action, NextAction::Continue));
}

#[tokio::test]
async fn free_prompting_backend_invoke_completes_without_tddy_tools_submit() {
    let recipe: Arc<dyn WorkflowRecipe> = Arc::new(FreePromptingRecipe);
    let backend: Arc<dyn CodingBackend> = Arc::new(OutputOnlyBackend);
    let task =
        BackendInvokeTask::from_recipe("prompting", GoalId::new("prompting"), recipe, backend);

    let ctx = Context::new();
    ctx.set_sync("feature_input", "hello");
    ctx.set_sync("output_dir", std::env::temp_dir());

    let result = task
        .run(ctx)
        .await
        .expect("task should succeed without submit");
    assert_eq!(result.response, "agent reply without submit");
    assert!(matches!(result.next_action, NextAction::Continue));
}

#[test]
fn free_prompting_prompting_goal_opted_out_of_tddy_tools_submit() {
    let recipe = FreePromptingRecipe;
    assert!(
        !recipe.goal_requires_tddy_tools_submit(&GoalId::new("prompting")),
        "prompting is open chat: recipe must not require structured submit"
    );
}

#[test]
fn grill_me_goal_opted_out_of_tddy_tools_submit() {
    let recipe = GrillMeRecipe;
    assert!(
        !recipe.goal_requires_tddy_tools_submit(&GoalId::new("grill")),
        "grill goal must not require structured submit"
    );
}

#[test]
fn tdd_plan_goal_still_requires_tddy_tools_submit() {
    let recipe = tddy_workflow_recipes::TddRecipe;
    assert!(
        recipe.goal_requires_tddy_tools_submit(&GoalId::new("plan")),
        "TDD plan must keep structured submit contract"
    );
}

#[test]
fn tdd_interview_goal_opted_out_of_tddy_tools_submit() {
    let recipe = TddRecipe;
    assert!(
        !recipe.goal_requires_tddy_tools_submit(&GoalId::new("interview")),
        "interview is elicitation: recipe must not require structured submit (grill-me parity)"
    );
}
